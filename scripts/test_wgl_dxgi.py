"""Check the picture of a fullscreen Windows game, presented through DXGI.

We compile scripts/native_runtime/test_wgl_dxgi.c with the fork's
gfx/drivers_context/wgl_dxgi.c and run it. We draw a picture with OpenGL into
the presenter, and a Windows capture of the window must show it upright, also
after a resize. The window stays outside the desktop. We present this way
only on Windows, and on macOS we show a fullscreen game through AppKit.

    python3 scripts/test_wgl_dxgi.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import menu_shots  # noqa: E402
import retroarch_probe  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "work/test-output/wgl-dxgi"


def windows() -> int:
    binary = retroarch_probe.build([ROOT / "scripts/native_runtime/test_wgl_dxgi.c"],
                                   ["gfx/drivers_context/wgl_dxgi.c"], OUTPUT,
                                   libraries=["-lopengl32", "-lgdi32", "-luser32"])
    return subprocess.run([str(binary)], timeout=60).returncode


def macos() -> int:
    print("macOS presents a fullscreen game through AppKit; nothing here to check")
    return 0


CHECKS = {"windows": windows, "macos": macos}


def main() -> int:
    if menu_shots.PLATFORM not in CHECKS:
        raise SystemExit(f"no DXGI presentation check is declared for {menu_shots.PLATFORM}")
    return CHECKS[menu_shots.PLATFORM]()


if __name__ == "__main__":
    sys.exit(main())
