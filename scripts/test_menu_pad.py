"""Compile RetroArch's reading of a controller for the menu from the fork, and
check what a held button is in the menu after a change to the game controls.

In test_menu_pad.c we call input_driver_collect_system_input, which we call
from the runloop once a frame, with the sources that we compile for the
keyboard test (test_menu_typing.py). We start nothing else of RetroArch, and
the program exits by itself.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import retroarch_probe  # noqa: E402
from test_menu_typing import DEFINES, FORK  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "work/test-output/menu-pad"


def main() -> int:
    binary = retroarch_probe.build([ROOT / "scripts/native_runtime/test_menu_pad.c"], FORK, OUTPUT, DEFINES)
    return subprocess.run([str(binary)]).returncode


if __name__ == "__main__":
    sys.exit(main())
