"""Test the hotkeys for use during play in the exported test player.

We export one generated Mega Drive cartridge with the builder's hotkeys and
play it, outside its menu, while we press keys with the menu's script driver
as a keyboard would (press:KEY). QUICK SAVE (F2) saves to the slot selected
in the menu, slot 1 before anyone chooses one, and the state file and picture
are then in the game's data. PREVIOUS SLOT (F6) and NEXT SLOT (F7) step the
slot round the six, the last before the first and the first after the last.
QUICK LOAD (F4) loads nothing from an empty slot and reports that, and loads
the saved one. The menu, when opened, shows the slot chosen with the hotkeys.
We read each step from the player's checkpoints: the words in the notice row,
the slots as in the menu, and the selection. In a second run, after we launch
the game again, we start on the slot chosen in the first run, and QUICK SAVE
saves there. That run ends on the notice, for its picture.

We run the test player of this checkout, built from the committed fork
(player_build.selected_build):
    python3 scripts/test_play_hotkeys.py [OUTPUT]

This does not prove that a key press on a physical keyboard reaches the menu
(in the script we hold the key where we read the keyboard for the menu),
sound, or window focus.
"""

from __future__ import annotations

import argparse
import os
import sys
from contextlib import ExitStack
from pathlib import Path

import exported_game
import menu_shots as shots
from make_test_rom import make_megadrive_rom
from menu_workflows import checkpoints

ROOT = shots.ROOT
OUTPUT = ROOT / "work/test-output/play-hotkeys"
# We play the game from the start and open the menu only at the end.
EXPORT = {"title": "Play Hotkeys", "startAtMenu": False, "splash": False}
# We hold a key down for two frames, then wait the frames that a slot change
# takes to show. We give a save or a load in RetroArch one second, which is
# enough for this cartridge. We read its notice while it is up, because a
# slot notice stays on the row only briefly (RIB_NOTICE, document_contract.inc).
STEP = "wait:6"
TASK = "wait-ms:1000"
SCRIPT = [
    "wait-ms:1000", "report:start",
    "press:f2", TASK, "report:saved",
    "press:f6", STEP, "report:previous",
    "press:f7", STEP, "report:wrapped",
    "press:f7", STEP, "press:f4", STEP, "report:empty",
    "press:f6", STEP, "press:f4", TASK, "report:loaded",
    "press:f7", STEP, "press:f7", STEP, "toggle", "wait-ms:500", "report:menu",
]
NOTICE_SCRIPT = ["wait-ms:1000", "press:f2", TASK, "report:relaunched"]


def notice(report: dict) -> str:
    return report["text"].get("unlock-title", "")


def state_files(data: Path) -> list[str]:
    """Return the save states and their pictures in the game's data, by name."""
    return sorted(path.name for path in data.rglob("*.state*") if path.is_file())


def files_in(data: Path) -> set[Path]:
    """Return every file in the game's data."""
    return {path for path in data.rglob("*") if path.is_file() and not path.is_symlink()}


def remove_new(data: Path, before: set[Path]) -> None:
    """Remove the files that a run wrote into the game's data (its states and
    the remembered slot), so that the next run starts from the same data as
    this one."""
    for path in files_in(data) - before:
        path.unlink()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, nargs="?", default=OUTPUT)
    arguments = parser.parse_args()
    shots.built_player()
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    rom = output / "play-hotkeys.md"
    rom.write_bytes(make_megadrive_rom())

    failures: list[str] = []

    def check(condition: bool, message: str) -> None:
        if not condition:
            failures.append(message)

    with ExitStack() as stack:
        app = stack.enter_context(shots.build_a_game(rom, output / "build", settings=EXPORT, namespace=".hotkeys"))
        data = exported_game.prepared_storage(app)
        if data is None:
            raise SystemExit("the exported game keeps no data of its own")
        before = state_files(data)
        if before:
            raise SystemExit(f"the game's data already holds states, which this does not remove: {before}")
        stack.callback(remove_new, data, files_in(data))

        failure = shots.take(app, "play-hotkeys", SCRIPT, output, reset_settings=True)
        if failure:
            raise SystemExit(f"the scripted run failed: {failure}")
        reports = checkpoints(output / "play-hotkeys.log")
        wanted = [step[len("report:"):] for step in SCRIPT if step.startswith("report:")]
        if list(reports) != wanted:
            raise SystemExit(f"expected checkpoints {wanted}, got {list(reports)}")

        check(notice(reports["start"]) == "", f"no notice before a hotkey: {notice(reports['start'])!r}")
        check(not reports["start"]["slots"][0]["occupied"], "slot 1 is empty at the start")
        saved = reports["saved"]
        check(notice(saved) == "SAVED TO SLOT 1", f"F2 says {notice(saved)!r}")
        # We write the picture after we report the save, and read it in the
        # menu the next time we read the slots, when the menu opens.
        check(saved["slots"][0]["occupied"], f"slot 1 holds the save: {saved['slots'][0]}")
        check(not any(slot["occupied"] for slot in saved["slots"][1:]), "no other slot holds a save")
        check(notice(reports["previous"]) == "SLOT 6", f"F6 from slot 1 says {notice(reports['previous'])!r}")
        check(notice(reports["wrapped"]) == "SLOT 1", f"F7 from slot 6 says {notice(reports['wrapped'])!r}")
        check(notice(reports["empty"]) == "SLOT 2 IS EMPTY", f"F4 on slot 2 says {notice(reports['empty'])!r}")
        check(notice(reports["loaded"]) == "LOADED SLOT 1", f"F4 on slot 1 says {notice(reports['loaded'])!r}")
        menu = reports["menu"]
        check(menu["menuOpen"], "the menu opened")
        check("slot-3" in menu["selected"] and "slot-1" not in menu["selected"],
              f"the menu shows slot 3 chosen: {menu['selected']}")
        check(menu["slots"][0] == {"occupied": True, "thumbnail": True},
              f"the menu shows slot 1's save with its picture: {menu['slots'][0]}")

        files = state_files(data)
        states = [name for name in files if name.endswith(".state1")]
        pictures = [name for name in files if name.endswith(".state1.png")]
        check(len(states) == 1 and len(pictures) == 1,
              f"slot 1's state and its picture are in the game's data: {files}")
        check(not [name for name in files if ".state1" not in name],
              f"no other slot's state was written: {files}")
        for name in pictures:
            picture = next(data.rglob(name))
            check(picture.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n", f"{name} is a PNG")

        # After the new launch, QUICK SAVE saves to slot 3, which was chosen
        # in the first run.
        failure = shots.take(app, "play-hotkeys-notice", NOTICE_SCRIPT, output, reset_settings=True)
        if failure:
            failures.append(f"the notice run failed: {failure}")
        else:
            relaunched = checkpoints(output / "play-hotkeys-notice.log")["relaunched"]
            check(notice(relaunched) == "SAVED TO SLOT 3",
                  f"F2 on the next launch says {notice(relaunched)!r}, not SAVED TO SLOT 3")
            check([name for name in state_files(data) if name.endswith(".state3")],
                  f"the next launch saved to slot 3: {state_files(data)}")

    for message in failures:
        print(f"FAIL {message}", file=sys.stderr)
    if failures:
        return 1
    print(f"the play hotkeys save, load and step the slot; pictures in {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
