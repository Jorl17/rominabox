"""The game's launcher as a plan tool, for the checks in which we read what we
write in a launch without starting a game.

With ROMINABOX_PLAN_ONLY we prepare the game's data folder and config in the
launcher and exit before a core is loaded. We build it for this machine and
put it where an exported game has its program, beside the folder with its
own files.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
import toolchain  # noqa: E402
from core_source import host_target  # noqa: E402


def compile_macos(directory: Path) -> tuple[Path, Path]:
    binary = directory / "Plan.app" / "Contents" / "MacOS" / "plan"
    binary.parent.mkdir(parents=True)
    made = subprocess.run(
        ["cc", "-DROMINABOX_PLAN_MAIN", "-O2", "-o", str(binary),
         *map(str, native_build.launcher_sources("macos"))],
        capture_output=True, text=True,
    )
    if made.returncode != 0:
        raise SystemExit(made.stderr[-600:] or "the launcher plan tool did not compile")
    return binary, binary.parents[1] / "Resources"


def compile_windows(directory: Path) -> tuple[Path, Path]:
    toolchain.activate()
    built = native_build.build_launcher(directory, host_target(), dict(os.environ))
    game = directory / "Plan"
    game.mkdir()
    binary = game / "Plan.exe"
    shutil.copy2(built, binary)
    return binary, game / "Resources"


# The plan tool, and the folder of its game's own files, on each platform.
PLANS = {"macos": compile_macos, "windows": compile_windows}


def compile_plan(directory: Path) -> tuple[Path, Path]:
    """The plan tool built in `directory`, and the folder of its game's own files."""
    platform = host_target().split("-", 1)[0]
    if platform not in PLANS:
        raise SystemExit(f"no launcher plan tool is declared for {platform}")
    return PLANS[platform](directory)
