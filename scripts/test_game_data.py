"""Compile the zips of a game's data with their test, from the sources the
launcher compiles (desktop/src-tauri/gamedata, the launcher's file layer,
miniz and rjson), and run it. The program exits by itself. On a Mac with zig we also
compile the sources for Windows, which shows only that the Windows code
compiles.

    uv run python scripts/test_game_data.py
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
LAUNCHER = ROOT / "desktop/src-tauri/launcher"
GAMEDATA = ROOT / "desktop/src-tauri/gamedata"
OUTPUT = ROOT / "work/test-output"


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    cc = toolchain.describe()["cc"]
    platform = "windows" if sys.platform == "win32" else "posix"
    includes = [LAUNCHER, LAUNCHER / platform, GAMEDATA, ROOT / "vendor/miniz", ROOT / "vendor/retroarch",
                native_build.LIBRETRO_COMMON / "include"]
    flags = [flag for folder in includes for flag in ("-I", str(folder))]
    # miniz and rjson are their libraries' own code, which we compile without
    # our warnings.
    objects = [OUTPUT / f"test_game_data_{library.stem}.o" for library in native_build.LIBRARIES]
    for library, compiled in zip(native_build.LIBRARIES, objects):
        subprocess.run([cc, "-std=gnu99", "-w", "-c", *flags, "-o", str(compiled), str(library)], check=True)
    binary = toolchain.executable(OUTPUT / "test_game_data")
    sources = [GAMEDATA / "game_data.c", LAUNCHER / "portable_fs.c", LAUNCHER / platform / "portable_fs.c",
               ROOT / "scripts/native_runtime/test_game_data.c"]
    subprocess.run([cc, "-std=gnu99", "-Wall", "-Wextra", "-Werror", *flags, "-o", str(binary),
                    *map(str, sources), *map(str, objects)], check=True)
    with tempfile.TemporaryDirectory(prefix="rominabox-game-data-") as folder:
        ran = subprocess.run([str(binary), folder]).returncode
    if ran != 0 or sys.platform == "win32":
        return ran
    zig = shutil.which("zig")
    if not zig:
        print("game data: Windows build not checked, zig is not installed")
        return 0
    windows = [LAUNCHER, LAUNCHER / "windows", GAMEDATA, ROOT / "vendor/miniz", ROOT / "vendor/retroarch",
               native_build.LIBRETRO_COMMON / "include"]
    windows_flags = [flag for folder in windows for flag in ("-I", str(folder))]
    for source, warnings in [(GAMEDATA / "game_data.c", ["-Wall", "-Wextra", "-Werror"]),
                             (LAUNCHER / "windows" / "portable_fs.c", ["-Wall", "-Wextra", "-Werror"]),
                             *((library, ["-w"]) for library in native_build.LIBRARIES)]:
        subprocess.run([zig, "cc", "-target", "x86_64-windows-gnu", "-std=gnu99", *warnings, *windows_flags,
                        "-c", str(source), "-o", str(OUTPUT / f"game-data-{source.stem}-windows.o")], check=True)
    print("game data: builds for Windows")
    return 0


if __name__ == "__main__":
    sys.exit(main())
