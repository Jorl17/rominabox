"""Compile RetroArch's reading of the keyboard for the menu from the fork, and
check what a held key is while someone types in the menu's text entry.

In test_menu_typing.c we call input_driver_collect_system_input, which we
call from the runloop once a frame. We start nothing else of RetroArch, and
the program exits by itself.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import retroarch_probe  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "work/test-output/menu-typing"
# input_driver.c and the small sources that it calls, as we compile them for
# the menu's reading of the keyboard in the player.
FORK = [
    "input/input_driver.c",
    "input/input_keymaps.c",
    "input/held_key_policy.c",
    "input/alt_enter_fullscreen.c",
    "libretro-common/compat/compat_strl.c",
    "libretro-common/string/stdstring.c",
    "libretro-common/encodings/encoding_utf.c",
    "libretro-common/file/file_path.c",
    "libretro-common/features/features_cpu.c",
    "libretro-common/time/rtime.c",
]
DEFINES = ["-DHAVE_MENU", "-DHAVE_RMLUI"]


def main() -> int:
    binary = retroarch_probe.build([ROOT / "scripts/native_runtime/test_menu_typing.c"], FORK, OUTPUT, DEFINES)
    return subprocess.run([str(binary)]).returncode


if __name__ == "__main__":
    sys.exit(main())
