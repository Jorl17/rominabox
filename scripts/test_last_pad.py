"""Compile RetroArch's rumble and pad reading from the fork, and check which pad
receives a core's rumble when every pad is player 1, and when each player
has a pad.

In test_last_pad.c we call input_set_rumble_state, the rumble interface for a
core, and input_driver_poll, with which we read the pads once a frame. We
start nothing else of RetroArch, and the program exits by itself.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import retroarch_probe  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "work/test-output/last-pad"
# input_driver.c and the small sources that it calls, compiled as for the player.
FORK = [
    "input/input_driver.c",
    "input/input_keymaps.c",
    "libretro-common/compat/compat_strl.c",
    "libretro-common/string/stdstring.c",
    "libretro-common/encodings/encoding_utf.c",
    "libretro-common/file/file_path.c",
]


def main() -> int:
    binary = retroarch_probe.build([ROOT / "scripts/native_runtime/test_last_pad.c"], FORK, OUTPUT)
    return subprocess.run([str(binary)]).returncode


if __name__ == "__main__":
    sys.exit(main())
