"""Build the player (the RetroArch fork with the RmlUi menu) into a new directory.

    uv run python scripts/build_player.py /absolute/new/build            # the host's target
    uv run python scripts/build_player.py --target windows-x86_64 DIR
    uv run python scripts/build_player.py --target macos-universal DIR   # the macOS kit's player

For a universal target we build each slice as its own target, in a folder
named after the slice inside DIR, and join their players into DIR/retroarch.
In the same build we make the game's launcher, where the recipe has one, and
the builder's menu preview renderer (rml-preview). A player for the tests that launch it has the menu's script driver
(ROMINABOX_MENU_SCRIPT_BUILD=1), and a player we ship does not. With either
test switch (the other is ROMINABOX_ACHIEVEMENTS_TEST_BUILD=1) the build is
test-only, and we never freeze it into a kit. What we build is in
scripts/native_runtime/player-recipe.json, and how in scripts/native_build.py.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import core_source  # noqa: E402
import native_build  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("destination", type=Path, help="a new, absolute directory")
    parser.add_argument("--target", default=core_source.host_target(), help="default: this machine's")
    arguments = parser.parse_args()
    target = native_build.require_build_target(arguments.target)
    destination: Path = arguments.destination
    if not destination.is_absolute():
        raise SystemExit("DESTINATION must be an absolute path")
    if destination.exists():
        raise SystemExit(f"refusing to overwrite existing destination: {destination}")

    commit = native_build.fork_commit()
    switches = {"achievements_test": os.environ.get("ROMINABOX_ACHIEVEMENTS_TEST_BUILD", "0"),
                "menu_script": os.environ.get("ROMINABOX_MENU_SCRIPT_BUILD", "0")}
    destination.mkdir(parents=True)
    parts = native_build.slices(target)
    if parts == [target]:
        build_slice(destination, target, commit, switches)
    else:
        for part in parts:
            build_slice(destination / part, part, commit, switches)
        join(destination, target, parts)
        write_info(destination, target, commit, switches, parts)
    print(f"Built {destination / 'retroarch' / native_build.binary_name(target)} for {target} from {commit}")
    return 0


def build_slice(destination: Path, target: str, commit: str, switches: dict[str, str]) -> None:
    """Build the player for one target in `destination`, with the launcher and
    the preview renderer that the recipe lists beside it."""
    jobs = os.cpu_count() or 1
    destination.mkdir(parents=True, exist_ok=True)
    native_build.archive_fork(commit, destination, target)
    rmlui_build = native_build.build_rmlui(destination, target, jobs)
    retroarch = destination / "retroarch"
    (retroarch / "Makefile.local").write_text(native_build.makefile_local(target), encoding="utf-8", newline="\n")
    accounts = native_build.copy_accounts(destination, target)

    environment = {**native_build.build_environment(target), **native_build.freetype_environment(destination)}
    native_build.run([*native_build.shell(target), "./configure", *native_build.configure_flags(target)],
                     retroarch, {**environment, **native_build.configure_environment(target)})
    rmlui = native_build.recipe()["rmlui"]
    native_build.run(["make", f"-j{jobs}",
                      f"RIB_ACHIEVEMENTS_TEST={switches['achievements_test']}",
                      f"RIB_MENU_SCRIPT={switches['menu_script']}",
                      f"RMLUI_SOURCE_DIR=../{rmlui['source']}", f"RMLUI_BUILD_DIR=../{rmlui_build.name}",
                      f"RIB_ACCOUNTS_DIR={native_build.make_path(accounts, target)}",
                      *native_build.make_variables(target)],
                     retroarch, environment)

    binary = retroarch / native_build.binary_name(target)
    for name, function in native_build.recipe()["capabilities"].items():
        if name != "comment" and not native_build.has_symbol(binary, target, function, environment):
            raise SystemExit(f"The built {target} player has no {name} ({function})")
    write_info(destination, target, commit, switches)
    native_build.run(["strip", str(binary)], retroarch, environment)
    launcher = native_build.build_launcher(destination, target, environment, retroarch)
    preview = native_build.build_preview(destination, target, environment, rmlui_build)
    for built in [binary, *([launcher] if launcher else []), *([preview] if preview else [])]:
        foreign = native_build.foreign_imports(built, target, environment)
        if foreign:
            raise SystemExit(f"{built.name} links libraries {target} does not have: {', '.join(foreign)}")
    others = [str(path) for path in (launcher, preview) if path]
    print(f"Built {binary}" + (f", and {', '.join(others)}" if others else ""), flush=True)


def join(destination: Path, target: str, parts: list[str]) -> None:
    """Join the players of the slices into one file, which must contain a slice
    for each processor, each linked only to the system's libraries. We checked
    the capabilities in every slice before we stripped it, because after
    stripping no symbol is left to read."""
    name = native_build.binary_name(target)
    joined = destination / "retroarch" / name
    joined.parent.mkdir()
    environment = native_build.build_environment(parts[0])
    native_build.run(["lipo", "-create", "-output", str(joined),
                      *(str(destination / part / "retroarch" / name) for part in parts)], destination, environment)
    wanted = {native_build.architecture_of(part) for part in parts}
    held = native_build.architectures_in(joined)
    if held != wanted:
        raise SystemExit(f"{joined} holds {sorted(a.value for a in held)}, not {sorted(a.value for a in wanted)}")
    for part in parts:
        foreign = native_build.foreign_imports(joined, part, environment)
        if foreign:
            raise SystemExit(f"The {part} slice of the joined player links {', '.join(foreign)}")
    # We sign every slice ad hoc, as macOS requires for arm64 code, because
    # linking leaves some slices unsigned.
    native_build.run(["codesign", "--force", "--sign", "-", str(joined)], destination, environment)


def write_info(destination: Path, target: str, commit: str, switches: dict[str, str],
               parts: list[str] | None = None) -> None:
    """Describe the build: its sources, its target (and the slices of a
    universal one), its capabilities, and whether it is only for tests."""
    info = {
        "retroarchCommit": commit, "rmluiCommit": native_build.recipe()["rmlui"]["commit"], "target": target,
        **({"slices": parts} if parts else {}),
        "capabilities": {"achievements": True, "menuScript": switches["menu_script"] == "1"},
        "testOnly": switches["achievements_test"] == "1" or switches["menu_script"] == "1",
    }
    (destination / "build-info.json").write_text(json.dumps(info) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    sys.exit(main())
