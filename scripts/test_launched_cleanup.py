"""Check that we remove what a game made when a launched test fails part-way.

A Windows game is one program. On its first launch we unpack it and
register a sandbox for it with Windows. When a harness stops between the
unpacking and its clean-up, the copy stays in the person's
%LOCALAPPDATA%\\ROM-in-a-Box\\Runtimes and the sandbox in their Packages
folder. In each case here
we make an export fail on purpose at such a point and then look for what
was left: anything new or changed in the person's ROM-in-a-Box folders, the
game's sandbox, and the temporary folders of the run.

    ROMINABOX_TEST_BUILD=/absolute/build python scripts/test_launched_cleanup.py

We do not check that a game plays (the launched tests), or what is left
after a Mac game, which we never unpack and whose data is in its container.
"""

from __future__ import annotations

import os
import sys
from collections.abc import Callable
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import menu_shots  # noqa: E402
import player_support  # noqa: E402
import scratch  # noqa: E402
import temp_entries  # noqa: E402
import test_shipped  # noqa: E402
import windows_pack  # noqa: E402
from launch_header import TEST_USER_DATA_ENV  # noqa: E402
from make_test_rom import make_megadrive_rom  # noqa: E402

FAILURES: list[str] = []


class MadeToFail(Exception):
    """The failure that we cause on purpose in a case."""


def check(condition: bool, message: str) -> None:
    print(f"  {'ok  ' if condition else 'FAIL'} {message}", flush=True)
    if not condition:
        FAILURES.append(message)


def failing_part_way(name: str, run: Callable[[Callable[[], None]], object], unpacks: list[Path]) -> None:
    """Run `run` with a step that fails, and check what is left of each game
    that it unpacked (`unpacks`) after the failure. Games from other checkouts
    run too, so we count only what is named for these games."""
    stamp = os.environ["ROMINABOX_SCRATCH_RUN"]
    temporary = temp_entries.snapshot()
    games: list[tuple[str, Path]] = []
    own: list[Path] = []

    def fail() -> None:
        for folder in unpacks:
            games.append((menu_shots.IDENTITY.search(menu_shots.plan_text(folder)).group(1),
                          menu_shots.storage_home(folder)))
        own.append(Path(os.environ[TEST_USER_DATA_ENV]))
        raise MadeToFail(name)

    try:
        run(fail)
        check(False, f"{name}: the step made to fail failed")
    except MadeToFail:
        pass
    check(bool(games), f"{name}: the game unpacked before the failure")
    person = player_support.user_data() / "ROM-in-a-Box"
    for identity, sandbox in games:
        left = sorted(str(path) for path in (person / "Runtimes").glob(f"{identity}-*"))
        left += [str(path) for path in (person / "Games" / identity, sandbox) if path.exists()]
        check(not left, f"{name}: nothing of {identity} is left" + (f": {', '.join(left)}" if left else ""))
    leftover = [entry for entry in temp_entries.additions(temporary, temp_entries.snapshot()) if stamp in entry]
    leftover += [str(folder) for folder in own if folder.exists()]
    check(not leftover, f"{name}: no temporary folder is left" + (f": {', '.join(leftover)}" if leftover else ""))


def main() -> int:
    if menu_shots.PLATFORM != "windows":
        print(f"{menu_shots.PLATFORM}: a game unpacks nothing and registers nothing outside its container")
        return 0
    if not os.environ.get("ROMINABOX_TEST_BUILD"):
        raise SystemExit("select the exact committed player with ROMINABOX_TEST_BUILD")
    os.environ.setdefault("ROMINABOX_SCRATCH_RUN", f"cleanup-{os.getpid()}")
    menu_shots.built_player()
    real_unpacked = windows_pack.unpacked
    unpacks: list[Path] = []

    def unpacking(program: Path, environment: dict[str, str]) -> Path:
        folder = real_unpacked(program, environment)
        unpacks.append(folder)
        return folder

    with scratch.scratch("rominabox-launched-cleanup-") as made, \
            mock.patch.object(windows_pack, "unpacked", unpacking):
        rom = Path(made) / "cleanup.md"
        rom.write_bytes(make_megadrive_rom())
        workspace = Path(made) / "workspace"

        def building(fail: Callable[[], None]) -> None:
            def unpack_then_fail(program: Path, environment: dict[str, str]) -> Path:
                unpacking(program, environment)
                fail()
                raise AssertionError("unreachable")

            with mock.patch.object(windows_pack, "unpacked", unpack_then_fail):
                with menu_shots.build_a_game(rom, workspace, settings={"title": "Cleanup While Building"}):
                    pass

        def playing(fail: Callable[[], None]) -> None:
            with menu_shots.build_a_game(rom, workspace, settings={"title": "Cleanup While Playing"}):
                fail()

        def shipping(fail: Callable[[], None]) -> None:
            with mock.patch.object(test_shipped, "compile_plan", side_effect=lambda _root: fail()):
                test_shipped.run_menu_sounds()

        for name, run in (("a game that fails as it is built", building),
                          ("a run that fails while its game is open", playing),
                          ("the shipped scope's menu sounds failing after the export", shipping)):
            unpacks.clear()
            failing_part_way(name, run, unpacks)
    if FAILURES:
        print(f"\n{len(FAILURES)} check(s) failed")
        return 1
    print("\nlaunched clean-up: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
