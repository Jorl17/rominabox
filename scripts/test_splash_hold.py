"""Check that the splash stays up for the time in the design, and that the
game waits for it.

We export the generated cartridge with the splash, set to open at its menu,
with the checkout's player (built with the menu's script driver), and start
it quietly. We draw the first frame of the menu while the game loads, with
the splash over it. The game starts, and a game set to open at its menu
opens it, only after the splash has gone. In the menu script we log both
moments: the first line on the menu's first frame, and the first step once
the menu is open. We time each line as it reaches the game's launch log, and
the time between them must be at least the splash time in the design (its
afterMs, holdMs and leaveMs) and not much more.

    python scripts/test_splash_hold.py

What it does not prove: the splash's appearance, that it appears in a window
(the window of a quiet run is hidden), or how long a launch by a person takes
to draw its first frame.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import exported_game  # noqa: E402
import menu_shots  # noqa: E402

DESIGN = ROOT / "integrations/designs/native/design.json"
# The verbose log lines for the menu's first frame and for the first step
# of the script.
SCRIPT_STARTS = "[RIB] menu script: "
FIRST_STEP = "[RIB] menu script waiting "
SCRIPT = "wait:1"
# A few frames before the splash begins and after the menu opens, on a
# machine busy with other tests. A splash kept up until the player's limit of
# three seconds goes past it.
SLACK_SECONDS = 1.0
TIMEOUT_SECONDS = 120


def declared_seconds() -> float:
    """Return the splash time in the design, during which the game waits: before
    the splash appears, while it is visible and while it leaves."""
    overlays = json.loads(DESIGN.read_text(encoding="utf-8"))["overlays"]
    holding = [overlay for overlay in overlays if overlay.get("holdsGame")]
    if len(holding) != 1:
        raise SystemExit(f"{DESIGN} declares {len(holding)} overlays that hold the game, not one")
    splash = holding[0]
    return (splash.get("afterMs", 0) + splash["holdMs"] + splash.get("leaveMs", 0)) / 1000


def timed_lines(app: Path, shot: Path) -> list[tuple[float, str]]:
    """Return each line of the launch log of one quiet run, with the seconds
    from the start of the run to the arrival of the line."""
    log = exported_game.log_of(app)
    log.unlink(missing_ok=True)
    environment = {**os.environ, exported_game.quiet_env(): "1", "ROMINABOX_VERBOSE": "1",
                   exported_game.SCRIPT_ENV: SCRIPT, "ROMINABOX_MENU_SHOT": str(shot)}
    started = time.perf_counter()
    player = subprocess.Popen([str(exported_game.launcher_of(app))], env=environment, stdin=subprocess.DEVNULL,
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    lines: list[tuple[float, str]] = []
    read = 0
    pending = b""
    while True:
        ended = player.poll() is not None
        if log.exists():
            with log.open("rb") as file:
                file.seek(read)
                arrived = file.read()
            now = time.perf_counter() - started
            read += len(arrived)
            *whole, pending = (pending + arrived).split(b"\n")
            lines += [(now, line.decode(errors="replace").rstrip("\r")) for line in whole]
        if ended:
            return lines
        if time.perf_counter() - started > TIMEOUT_SECONDS:
            raise menu_shots.PlayerTimeout(
                f"the game did not close within {TIMEOUT_SECONDS}s (pid {player.pid}); left running for inspection", app)
        time.sleep(0.002)


def main() -> int:
    import fetch_test_content

    cartridge, reason = fetch_test_content.locate("test-game")
    if cartridge is None:
        print(f"skipped: test-game not available ({reason})")
        return 0
    declared = declared_seconds()
    settings = {"title": "Splash Hold", "startAtMenu": True, "splash": True}
    with menu_shots.build_a_game(cartridge, ROOT / "work/splash-hold", "gbc", settings) as app:
        picture = ROOT / "work/test-output/splash-hold.png"
        shot = exported_game.shot_inside(app, picture)
        lines = timed_lines(app, shot)
        exported_game.carry_shot(shot, picture)
    starts = next((when for when, line in lines if SCRIPT_STARTS in line), None)
    step = next((when for when, line in lines if FIRST_STEP in line), None)
    if starts is None or step is None:
        print("\n".join(line for _, line in lines[-20:]))
        print("FAIL the log does not say when the menu's first frame was and when the menu opened")
        return 1
    held = step - starts
    print(f"{exported_game.PLATFORM}: the menu opened {held:.3f}s after its first frame; "
          f"the design keeps the splash up {declared:.3f}s")
    if not declared <= held <= declared + SLACK_SECONDS:
        print(f"FAIL the game waited {held:.3f}s for a splash declared for {declared:.3f}s")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
