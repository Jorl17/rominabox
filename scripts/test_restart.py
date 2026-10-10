"""Test RESTART in the exported test player.

We export one generated Mega Drive cartridge, play it, open the menu, choose
RESTART and confirm it. RetroArch resets the console, and the menu closes, so
the player is back in the game, which starts again. When the player opens the
menu again, it opens on the pause screen. We read the reset from the player's
log, and the menu from the player's checkpoints.

We run the test player of this checkout, built from the committed fork
(player_build.selected_build):
    uv run python scripts/test_restart.py [OUTPUT]

This does not prove what the game draws after the reset, sound, or window
focus.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

import menu_shots as shots
from make_test_rom import make_megadrive_rom
from menu_workflows import checkpoints

ROOT = shots.ROOT
OUTPUT = ROOT / "work/test-output/restart"
DESIGN = ROOT / "integrations/designs/native/design.json"
# We play the game from the start, as an exported game does by default.
EXPORT = {"title": "Restart", "startAtMenu": False, "splash": False}
# What RetroArch logs when it resets the console.
RESET = "[Core] Reset."


def button(role: str) -> str:
    """The id of the button that opens the screen with `role` in the design."""
    for screen in json.loads(DESIGN.read_text(encoding="utf-8"))["screens"]:
        if screen.get("role") == role:
            return screen["button"]
    raise SystemExit(f"the design has no screen for {role}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, nargs="?", default=OUTPUT)
    arguments = parser.parse_args()
    shots.built_player()
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    rom = output / "restart.md"
    rom.write_bytes(make_megadrive_rom())
    script = [
        "wait-ms:1000", "toggle", "wait-ms:500", "report:open",
        button("restart"), "report:asked",
        "restart-confirm", "wait-ms:500", "report:restarted",
        "toggle", "wait-ms:500", "report:reopened",
    ]
    # RetroArch logs the reset only when it logs everything.
    os.environ["ROMINABOX_VERBOSE"] = "1"

    with shots.build_a_game(rom, output / "build", settings=EXPORT, namespace=".restart") as app:
        failure = shots.take(app, "restart", script, output, reset_settings=True)
    if failure:
        raise SystemExit(f"the scripted run failed: {failure}")
    log = (output / "restart.log").read_text(encoding="utf-8", errors="replace")
    reports = checkpoints(output / "restart.log")
    wanted = [step[len("report:"):] for step in script if step.startswith("report:")]
    if list(reports) != wanted:
        raise SystemExit(f"expected checkpoints {wanted}, got {list(reports)}")

    failures: list[str] = []

    def check(condition: bool, message: str) -> None:
        if not condition:
            failures.append(message)

    check(reports["open"]["menuOpen"] and reports["open"]["screen"] == "pause",
          f"the menu opens on the pause screen: {reports['open']['screen']}")
    check(reports["asked"]["screen"] == "restart", f"RESTART asks first: {reports['asked']['screen']}")
    check(RESET in log, "the console is reset")
    check(not reports["restarted"]["menuOpen"],
          f"the menu closes after the restart, and shows {reports['restarted']['screen']}")
    check(reports["reopened"]["menuOpen"] and reports["reopened"]["screen"] == "pause",
          f"the menu opens again on the pause screen: {reports['reopened']['screen']}")
    for message in failures:
        print(f"FAIL: {message}", file=sys.stderr)
    if failures:
        return 1
    print("RESTART resets the console, closes the menu, and the menu opens again on the pause screen.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
