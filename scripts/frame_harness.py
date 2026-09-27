"""The frame harness, scripts/native_runtime/frame_harness.c, with which we run
an actual core with no window, no audio device and no RetroArch. We compile it
here, with this machine's declared toolchain, for every script that uses it.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "scripts/native_runtime/frame_harness.c"
LIBRETRO = ROOT / "vendor/retroarch/libretro-common/include"


def compile_to(destination: Path) -> Path:
    """Compile the harness as `destination`, and return the program written,
    which has the platform's executable suffix."""
    declared = toolchain.describe()
    program = toolchain.executable(destination)
    compiled = subprocess.run(
        [declared["cc"], "-O2", f"-I{LIBRETRO}", "-o", str(program), str(SOURCE),
         *declared["supportLibraries"]],
        capture_output=True,
        text=True,
        timeout=120,
    )
    if compiled.returncode != 0:
        sys.stderr.write(compiled.stderr)
        raise SystemExit("frame_harness did not compile")
    return program
