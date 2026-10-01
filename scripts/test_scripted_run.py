"""Check that a run driven by a menu script has no controller.

We control what such a run shows and does only through the script, whatever
pads the machine has. Otherwise a connected DualSense would add its bindings
to the workflow pictures on one machine and not on another. We export the
generated cartridge with the checkout's player and start it quietly for one
frame, once with a script and once without. The log of each run contains
the controller driver started in RetroArch, whatever is plugged in. We
expect RetroArch's null driver for the scripted run, and the platform's
driver for the other.

    python3 scripts/test_scripted_run.py
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import exported_game  # noqa: E402
import menu_shots  # noqa: E402

# The verbose log line for each controller driver started in RetroArch.
STARTED = re.compile(r'Found joypad driver: "([^"]+)"')
# The RetroArch controller driver without controllers.
NO_CONTROLLERS = "null"
# A script with a wait of one frame. Without the script driver in the player,
# the script has no effect, and the launch plan is the same either way.
SCRIPT = "wait:1"


def started_drivers(launcher: Path, log: Path, script: str | None) -> list[str]:
    """Return the controller drivers started in a one-frame run, in log order."""
    if log.exists():
        log.unlink()
    env = {**os.environ, "ROMINABOX_MAX_FRAMES": "1", "ROMINABOX_VERBOSE": "1",
           exported_game.quiet_env(): "1"}
    env.pop(exported_game.SCRIPT_ENV, None)
    if script is not None:
        env[exported_game.SCRIPT_ENV] = script
    code = subprocess.run([str(launcher)], timeout=60, env=env).returncode
    text = log.read_text(encoding="utf-8", errors="replace") if log.exists() else ""
    if code != 0 or "[Content] Loading content file" not in text:
        print("\n".join(text.splitlines()[-20:]))
        raise SystemExit(f"a one-frame run did not load the game and exit 0 (exit {code})")
    return STARTED.findall(text)


def main() -> int:
    import fetch_test_content

    cartridge, reason = fetch_test_content.locate("test-game")
    if cartridge is None:
        print(f"skipped: test-game not available ({reason})")
        return 0
    settings = {"title": "Scripted Run", "startAtMenu": False}
    with menu_shots.build_a_game(cartridge, ROOT / "work/scripted-run", "gbc", settings) as app:
        launcher = exported_game.launcher_of(app)
        log = exported_game.log_of(app)
        if log is None:
            raise SystemExit(f"the export at {app} names no data folder for its log")
        scripted = started_drivers(launcher, log, SCRIPT)
        plain = started_drivers(launcher, log, None)
    failures = []
    if scripted != [NO_CONTROLLERS]:
        failures.append(f"a run {exported_game.SCRIPT_ENV} drives started controller driver(s) {scripted}, "
                        f"not only {NO_CONTROLLERS!r}")
    if not plain or NO_CONTROLLERS in plain:
        failures.append(f"a run without a menu script started controller driver(s) {plain}, "
                        "not the platform's own")
    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print(f"{exported_game.PLATFORM}: a scripted run starts {scripted[0]!r}; a plain one {plain[0]!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
