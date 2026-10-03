"""Compile the RetroArch SDL2 joypad driver of the Mac player from the fork,
with a stand-in for SDL, and record the calls to SDL when a core sets the two
motors of a pad one after the other (native_runtime/sdl2_rumble.c).

The SDL headers are from the release pinned in the player recipe, which we
unpack here once. We do not link SDL itself, and no part of RetroArch starts.

    uv run python scripts/test_sdl2_rumble.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
import retroarch_probe  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "work/test-output/sdl2-rumble"


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    headers = native_build.sdl2_source(OUTPUT) / "include"
    binary = retroarch_probe.build([ROOT / "scripts/native_runtime/sdl2_rumble.c"],
                                   ["input/drivers_joypad/sdl2_joypad.c"], OUTPUT,
                                   ["-DHAVE_SDL2", f"-I{headers}"])
    return subprocess.run([str(binary)]).returncode


if __name__ == "__main__":
    sys.exit(main())
