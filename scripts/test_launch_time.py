"""Measure how long an exported game takes to start on this platform.

We export the generated cartridge with the player of this checkout and start
it six times as a harness does, quietly and for one frame. The wall time of
a run, from the start of the program to its exit after that frame, is an
upper bound on the time to the first frame, because it includes shutdown.
We report the first run apart from the median of the others, because the
first run also loads the programs from disk. We set no time limit yet, and
fail only when a run does not start and exit cleanly.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import exported_game  # noqa: E402
import menu_shots  # noqa: E402

RUNS = 6


def main() -> int:
    import fetch_test_content

    cartridge, reason = fetch_test_content.locate("test-game")
    if cartridge is None:
        print(f"skipped: test-game not available ({reason})")
        return 0
    settings = {"title": "Launch Time", "startAtMenu": False}
    with menu_shots.build_a_game(cartridge, ROOT / "work/launch-time", "gbc", settings) as app:
        launcher = exported_game.launcher_of(app)
        log = exported_game.log_of(app)
        times: list[float] = []
        for _ in range(RUNS):
            if log and log.exists():
                log.unlink()
            started = time.perf_counter()
            code = subprocess.run(
                [str(launcher)],
                timeout=60,
                # Verbose, so that each run's log shows the game loaded. We write
                # no such lines when a person launches the game.
                env={**os.environ, "ROMINABOX_MAX_FRAMES": "1", "ROMINABOX_VERBOSE": "1",
                     exported_game.quiet_env(): "1"},
            ).returncode
            times.append(time.perf_counter() - started)
            text = log.read_text(errors="replace") if log and log.exists() else ""
            if code != 0 or "[Content] Loading content file" not in text:
                print("\n".join(text.splitlines()[-20:]))
                print(f"a one-frame run did not load the game and exit 0 (exit {code})")
                return 1
    warm = sorted(times[1:])
    print(
        f"{exported_game.PLATFORM}: first run {times[0]:.2f} s; then median {warm[len(warm) // 2]:.2f} s "
        f"(from {warm[0]:.2f} to {warm[-1]:.2f} s over {len(warm)} runs)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
