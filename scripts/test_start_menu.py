"""Test that a game set to show its menu at startup shows it before it runs.

We export one generated Mega Drive cartridge with the splash, the menu at
startup and autosave on quit. On the first launch the menu opens before the
game runs its first frame, and the game runs once the player continues. That
run saves the game when it ends, so on the second launch the game loads its
autosave, and the menu again opens before the first frame. We read the order
from the player's log: the menu's checkpoints, and the line RetroArch logs when
the game runs its first frame.

We run the test player of this checkout, built from the committed fork
(player_build.selected_build):
    uv run python scripts/test_start_menu.py [OUTPUT]

This does not prove what is drawn, how long the splash lasts, sound, or window
focus.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

import menu_shots as shots
from make_test_rom import make_megadrive_rom
from menu_workflows import checkpoints

ROOT = shots.ROOT
OUTPUT = ROOT / "work/test-output/start-menu"
EXPORT = {"title": "Start Menu", "startAtMenu": True, "splash": True, "autosaveOnQuit": True}
# What RetroArch logs when the game runs its first frame, and when it loads the
# autosave.
FIRST_FRAME = "[Core] First frame."
AUTOSAVE_LOADED = "Auto-loading save state from"


def run(app: Path, name: str, script: list[str], output: Path) -> tuple[list[str], dict[str, dict]]:
    """The lines of the run's log, and its checkpoints."""
    failure = shots.take(app, name, script, output, reset_settings=True)
    if failure:
        raise SystemExit(f"the {name} run failed: {failure}")
    log = output / f"{name}.log"
    reports = checkpoints(log)
    wanted = [step[len("report:"):] for step in script if step.startswith("report:")]
    if list(reports) != wanted:
        raise SystemExit(f"{name}: expected checkpoints {wanted}, got {list(reports)}")
    return log.read_text(encoding="utf-8", errors="replace").splitlines(), reports


def line_of(lines: list[str], text: str) -> int | None:
    return next((index for index, line in enumerate(lines) if text in line), None)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, nargs="?", default=OUTPUT)
    arguments = parser.parse_args()
    shots.built_player()
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    rom = output / "start-menu.md"
    rom.write_bytes(make_megadrive_rom())
    # RetroArch logs the first frame and the autosave only when it logs everything.
    os.environ["ROMINABOX_VERBOSE"] = "1"

    failures: list[str] = []

    def check(condition: bool, message: str) -> None:
        if not condition:
            failures.append(message)

    def menu_before_first_frame(lines: list[str], reports: dict[str, dict], label: str) -> None:
        check(reports[label]["menuOpen"], f"{label}: the menu is open")
        menu = line_of(lines, f"[RIB] checkpoint {label} ")
        first = line_of(lines, FIRST_FRAME)
        check(menu is not None and (first is None or menu < first),
              f"{label}: the menu opens before the game runs its first frame "
              f"(checkpoint at line {menu}, first frame at line {first})")

    with shots.build_a_game(rom, output / "build", settings=EXPORT, namespace=".startmenu") as app:
        lines, reports = run(app, "start-menu-first", [
            "report:started", "toggle", "wait-ms:1000", "report:playing",
        ], output)
        menu_before_first_frame(lines, reports, "started")
        check(not reports["playing"]["menuOpen"], "the menu closes when the player continues")
        check(line_of(lines, FIRST_FRAME) is not None, "the game runs once the player continues")

        lines, reports = run(app, "start-menu-resumed", ["report:resumed"], output)
        check(line_of(lines, AUTOSAVE_LOADED) is not None and "succeeded" in lines[line_of(lines, AUTOSAVE_LOADED)],
              "the second launch loads the autosave of the first")
        menu_before_first_frame(lines, reports, "resumed")

    for message in failures:
        print(f"FAIL: {message}", file=sys.stderr)
    if failures:
        return 1
    print("The menu opens before the game's first frame, on a first launch and when it resumes its autosave.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
