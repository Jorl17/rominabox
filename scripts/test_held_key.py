"""Compile the menu-toggle and Alt+Enter decisions with their tests, and run them.

In the tests we hold one key, press another, and call the same functions
as the runloop. Each program exits by itself.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
PLAYER = ROOT / "vendor/retroarch"
OUTPUT = ROOT / "work/test-output"

# (program, the decision it tests, its test)
PROGRAMS = [
    ("test_held_key", PLAYER / "input/held_key_policy.c", ROOT / "scripts/native_runtime/test_held_key.c"),
    ("test_alt_enter", PLAYER / "input/alt_enter_fullscreen.c", ROOT / "scripts/native_runtime/test_alt_enter.c"),
]


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    cc = toolchain.describe()["cc"]
    for name, decision, test in PROGRAMS:
        binary = toolchain.executable(OUTPUT / name)
        subprocess.run([cc, "-Wall", "-Werror", "-I", str(PLAYER), "-o", str(binary), str(decision), str(test)],
                       check=True)
        ran = subprocess.run([str(binary)])
        if ran.returncode != 0:
            return ran.returncode
    return 0


if __name__ == "__main__":
    sys.exit(main())
