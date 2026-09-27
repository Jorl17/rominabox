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
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
PLAYER = ROOT / "vendor/retroarch"
OUTPUT = ROOT / "work/test-output/menu-typing"
TESTS = [
    ROOT / "scripts/native_runtime/test_menu_typing.c",
    ROOT / "scripts/native_runtime/test_menu_typing_unreached.c",
]
# input_driver.c and the small sources that it calls, as we compile them for
# the menu's reading of the keyboard in the player.
FORK = [PLAYER / path for path in (
    "input/input_driver.c",
    "input/input_keymaps.c",
    "input/held_key_policy.c",
    "input/alt_enter_fullscreen.c",
    "libretro-common/compat/compat_strl.c",
    "libretro-common/string/stdstring.c",
    "libretro-common/encodings/encoding_utf.c",
    "libretro-common/file/file_path.c",
)]
DEFINES = ["-DHAVE_MENU", "-DHAVE_RMLUI"]
# Drop uncalled code before resolving names, with the linkers that can do it.
UNUSED = {
    "darwin": ["-Wl,-dead_strip"],
    "linux": ["-Wl,--gc-sections"],
    "win32": ["-Wl,--gc-sections"],
}


def main() -> int:
    toolchain.activate()
    cc = toolchain.describe()["cc"]
    OUTPUT.mkdir(parents=True, exist_ok=True)
    # input_driver.h includes "../config.h", which comes from RetroArch's
    # configure script. The code that we read here does not use it, so we put
    # an empty one in its place, one directory above an include path of its own.
    configured = OUTPUT / "configured"
    (configured / "include").mkdir(parents=True, exist_ok=True)
    (configured / "config.h").write_text("", encoding="utf-8")
    includes = [f"-I{configured / 'include'}", f"-I{PLAYER}",
                f"-I{PLAYER / 'libretro-common/include'}", f"-I{PLAYER / 'deps'}"]
    objects = []
    for source in FORK + TESTS:
        warnings = ["-w"] if source in FORK else ["-Wall", "-Werror"]
        built = OUTPUT / f"{source.stem}.o"
        subprocess.run([cc, "-std=gnu99", "-ffunction-sections", "-fdata-sections", *warnings,
                        *DEFINES, *includes, "-c", str(source), "-o", str(built)], check=True)
        objects.append(str(built))
    binary = toolchain.executable(OUTPUT / "test_menu_typing")
    subprocess.run([cc, *UNUSED[sys.platform], *objects, "-o", str(binary)], check=True)
    return subprocess.run([str(binary)]).returncode


if __name__ == "__main__":
    sys.exit(main())
