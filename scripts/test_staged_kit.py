"""Check what we build a game from in a launched test: the kit staged from
this tree, and the game's launcher, compiled once.

In a launched test we export the games from a copy of the runtime kit, with
the player and the launcher from this checkout (menu_shots.staged_kit), and
in the shipped tests we run the launcher as a plan tool (launcher_plan). We
compile the launcher at most once per run, because each compile takes about
three seconds on Windows. We also stage the shared parts and the shader
library from the tree, as well as the menu designs, so that a change to a
part appears in a shot without a new build of the kit.

    python3 scripts/test_staged_kit.py

What this does NOT prove: that a game exported from the kit runs (the
launched tests check that), or that we compile a change to the launcher's
sources (that depends on ninja and on the kit's record of the sources of
its launch library).
"""

from __future__ import annotations

import filecmp
import os
import shutil
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import kit_assets  # noqa: E402
import launcher_plan  # noqa: E402
import menu_shots  # noqa: E402
import native_build  # noqa: E402
import scratch  # noqa: E402
from core_source import host_target  # noqa: E402

FAILURES: list[str] = []


def check(condition: bool, message: str) -> None:
    print(f"  {'ok  ' if condition else 'FAIL'} {message}", flush=True)
    if not condition:
        FAILURES.append(message)


def compiled_by(action: Callable[[], object]) -> list[str]:
    """Return the launcher sources compiled while `action` ran, from the build
    output, with each command from native_build and each step from ninja."""
    sys.stdout.flush()
    sys.stderr.flush()
    saved = os.dup(1), os.dup(2)
    with tempfile.TemporaryFile() as captured:
        os.dup2(captured.fileno(), 1)
        os.dup2(captured.fileno(), 2)
        try:
            action()
        finally:
            sys.stdout.flush()
            sys.stderr.flush()
            os.dup2(saved[0], 1)
            os.dup2(saved[1], 2)
            os.close(saved[0])
            os.close(saved[1])
        captured.seek(0)
        said = captured.read().decode("utf-8", errors="replace")
    platform = native_build.platform_of(host_target())
    return [source.name for source in native_build.launcher_sources(platform) if str(source) in said]


def different(staged: Path, tree: Path) -> list[str]:
    """Return the differences between the folder `staged` and `tree`: each file
    missing, extra or with other bytes, by its path in the folder."""
    found = []
    compared = filecmp.dircmp(staged, tree)
    found += [f"{name} is not in the tree" for name in compared.left_only]
    found += [f"{name} is missing" for name in compared.right_only]
    found += [f"{name} differs" for name in compared.diff_files]
    for name, below in compared.subdirs.items():
        found += [f"{name}/{entry}" for entry in different(Path(below.left), Path(below.right))]
    return found


def a_staged_kit_carries_the_trees_parts_and_shaders(root: Path) -> None:
    """Check that we stage a kit with older parts and shaders with those from the
    tree: the parts linked in composition and the filters in an export."""
    stale = root / "stale-kit"
    shutil.copytree(menu_shots.KIT, stale, symlinks=True)
    for folder in ("parts", "shaders"):
        (stale / folder / "retired.rcss").write_text("/* a file the tree no longer has */\n", encoding="utf-8")
    navigation = stale / "parts" / "navigation.rcss"
    navigation.write_text(navigation.read_text(encoding="utf-8") + "/* older */\n", encoding="utf-8")
    player = stale / native_build.kit_file(host_target(), "player")
    with mock.patch.object(menu_shots, "KIT", stale):
        kit = menu_shots.staged_kit(root / "parts" / "kit", player)
    for folder, tree in (("parts", kit_assets.PARTS), ("shaders", kit_assets.SHADERS)):
        left = different(kit / folder, tree)
        check(not left, f"the staged kit's {folder} are the tree's" + (f": {', '.join(left)}" if left else ""))


def a_second_kit_compiles_no_launcher(root: Path) -> None:
    """Check that in a run with several exports we compile the launcher at most
    for the first, and stage nothing beside the kit."""
    player = menu_shots.KIT / native_build.kit_file(host_target(), "player")
    menu_shots.staged_kit(root / "first" / "kit", player)
    second = root / "second" / "kit"
    compiled = compiled_by(lambda: menu_shots.staged_kit(second, player))
    check(not compiled, "staging a second kit compiles no launcher"
          + (f"; it compiled {', '.join(compiled)}" if compiled else ""))
    beside = sorted(path.name for path in second.parent.iterdir() if path != second)
    check(not beside, "staging a kit writes nothing beside it" + (f": {', '.join(beside)}" if beside else ""))


def a_second_plan_tool_compiles_no_launcher(root: Path) -> None:
    """Check that in the shipped tests, where each case has a plan tool, we
    compile the launcher at most for the first."""
    first = root / "plan-first"
    first.mkdir()
    launcher_plan.compile_plan(first)
    second = root / "plan-second"
    second.mkdir()
    made: list[tuple[Path, Path]] = []
    compiled = compiled_by(lambda: made.append(launcher_plan.compile_plan(second)))
    check(not compiled, "a second plan tool compiles no launcher"
          + (f"; it compiled {', '.join(compiled)}" if compiled else ""))
    binary = made[0][0] if made else None
    written = sorted(path.relative_to(second).as_posix() for path in second.rglob("*")
                     if path.is_file() and path != binary)
    check(binary is not None and binary.is_file() and not written,
          "a plan tool's folder holds the tool and nothing else" + (f": {', '.join(written)}" if written else ""))


def main() -> int:
    if not (menu_shots.KIT / native_build.kit_file(host_target(), "player")).is_file():
        raise SystemExit(f"no runtime kit at {menu_shots.KIT}; make one with scripts/build_kit.py")
    with scratch.scratch("rominabox-staged-kit-") as made:
        root = Path(made)
        a_staged_kit_carries_the_trees_parts_and_shaders(root)
        a_second_kit_compiles_no_launcher(root)
        a_second_plan_tool_compiles_no_launcher(root)
    if FAILURES:
        print(f"\n{len(FAILURES)} check(s) failed")
        return 1
    print("\nstaged kit: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
