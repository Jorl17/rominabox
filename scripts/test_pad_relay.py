"""Test the controller relay of a sandboxed Windows game with DirectInput.

    uv run python scripts/test_pad_relay.py

We compile the launcher side (`desktop/src-tauri/launcher/windows/pad_relay.c`)
and the DirectInput stand-in of the player
(`vendor/retroarch/input/drivers/rominabox_dinput.c`) with
`scripts/pad_relay_test.c` and run it. The game must list the controllers
that DirectInput lists outside and every other device as DirectInput lists
it. We set up a controller, read it within the range that the game set,
send it rumble through the launcher, and stop answering in the launcher,
after which the game must lose its controllers. We skip what requires a
controller when none is connected, and report that. In the launcher we send
rumble through SDL when DirectInput cannot, so we link its side with the
recipe's SDL2, built in the launcher build folder of this checkout as for the
launcher. The relay is only for Windows, and elsewhere we only check that
both sides build for it with zig, against SDL's own headers.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import core_source  # noqa: E402
import native_build  # noqa: E402
import toolchain  # noqa: E402

PLAYER = ROOT / "vendor/retroarch"
LAUNCHER = ROOT / "desktop/src-tauri/launcher/windows"
OUTPUT = ROOT / "work/test-output"
RELAY = [LAUNCHER / "pad_relay.c", PLAYER / "input/drivers/rominabox_dinput.c"]
# In the stand-in we read the relay's name as we read the environment in the
# player, through libretro's UTF-8 conversion.
SOURCES = [ROOT / "scripts/pad_relay_test.c", *RELAY, PLAYER / "libretro-common/encodings/encoding_utf.c",
           PLAYER / "libretro-common/compat/compat_strl.c"]
INCLUDES = [f"-I{LAUNCHER}", f"-I{PLAYER}", f"-I{PLAYER / 'libretro-common/include'}"]
WARNINGS = ["-Wall", "-Wextra", "-Werror"]
WINDOWS = "windows-x86_64"


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    if core_source.host_target().startswith("windows-"):
        toolchain.activate()
        binary = toolchain.executable(OUTPUT / "pad-relay-test")
        with native_build.tree_build(native_build.kit_target(WINDOWS)) as folder:
            native_build.build_sdl2(folder, WINDOWS, os.cpu_count() or 1)
            subprocess.run(
                [toolchain.describe()["cc"], "-std=gnu99", "-O1", "-g", *WARNINGS, *INCLUDES,
                 native_build.sdl2_include(folder), *map(str, SOURCES), "-o", str(binary),
                 "-lSDL2", *native_build.sdl2_libraries(folder), "-ldinput8", "-ldxguid", "-lole32", "-luser32"],
                check=True,
            )
        subprocess.run([str(binary)], check=True)
        return 0
    zig = shutil.which("zig")
    if not zig:
        print("pad relay: Windows's own; not built, zig is not installed")
        return 0
    with native_build.tree_build(native_build.kit_target(WINDOWS)) as folder:
        headers = native_build.sdl2_source(folder) / "include"
        for source in RELAY:
            subprocess.run(
                [zig, "cc", "-target", "x86_64-windows-gnu", "-std=gnu99", *WARNINGS, *INCLUDES,
                 f"-I{headers}", "-c", str(source), "-o", str(OUTPUT / f"{source.stem}-windows.o")],
                check=True,
            )
    print("pad relay: builds for Windows")
    return 0


if __name__ == "__main__":
    sys.exit(main())
