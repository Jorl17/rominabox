"""Check the menu drawing in an OpenGL core context, and log lines in a file.

The Flycast core runs in an OpenGL core context, where nothing appears when
we draw with client arrays, as RmlUi's GL2 backend does. A Dreamcast game
would then have no pause menu. We must still draw in a legacy context too.

In RmlUi, a box-shadow is drawn into a layer that is then used as a texture.
Our GL2 backend had no layers, so every element with a shadow appeared as a
white block, with its shadow over the top-left corner of the window.

The standard output of the player is a file. When that output is fully
buffered, RetroArch's lines stay in the buffer until the process exits,
and they are lost when someone kills the player.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
# We take the headers and the archive from the player build, because a
# literal path here could point to a folder that no script creates.
import native_build  # noqa: E402
import toolchain  # noqa: E402
import rmlui_paths  # noqa: E402
from rmlui_paths import HEADER_DIRS, LIBRARY  # noqa: E402
sys.path.insert(0, str(ROOT / "scripts/native_runtime"))
# We read the renderer's pictures through libretro's file layer.
import menu_harness  # noqa: E402

RETROARCH = ROOT / "vendor/retroarch"
DRIVERS = RETROARCH / "menu/drivers"
NATIVE = ROOT / "scripts/native_runtime"
PROBE = ROOT / "work/dcmenu-probe"
# The place in the macOS launcher where we point its output at launch.log.
LAUNCHER_POSIX = ROOT / "desktop/src-tauri/launcher/posix"
LINE = PROBE / "line.txt"


def check_log_line() -> bool:
    if native_build.is_windows(rmlui_paths.TARGET):
        # In the macOS launcher we point the output of the launcher process
        # at the log. On Windows the player runs in a separate process with a
        # log handle from its launcher, and we flush each line in RetroArch's
        # logger. We do not test the Windows case here.
        print("not on Windows: this checks the macOS launcher's in-process log redirection")
        return True
    PROBE.mkdir(parents=True, exist_ok=True)
    binary = PROBE / "log_lines"
    compiled = subprocess.run(
        ["cc", "-Wall", "-Wextra", "-Werror", f"-I{LAUNCHER_POSIX}", "-o", str(binary),
         str(NATIVE / "log_lines_probe.c"), str(LAUNCHER_POSIX / "log_output.c")],
        capture_output=True, text=True,
    )
    if compiled.returncode != 0:
        print(compiled.stderr)
        print("FAIL log line check did not compile")
        return False
    if LINE.exists():
        LINE.unlink()
    ran = subprocess.run([str(binary), str(LINE)], capture_output=True, text=True, timeout=20)
    sys.stdout.write(ran.stdout)
    sys.stderr.write(ran.stderr)
    written = LINE.read_text() if LINE.exists() else ""
    if LINE.exists():
        LINE.unlink()
    if ran.returncode != 0:
        print(f"FAIL the log line probe exited with {ran.returncode}")
        return False
    if "menu line" not in written:
        print("FAIL a log line never reached the file")
        return False
    print("ok a log line reached the file before exit")
    return True


def probe_platform() -> tuple[list[Path], list[Path], list[str]]:
    """Return the OpenGL context in which we draw in the probe, which is the
    context of the builder's preview renderer as declared in the recipe for
    this target: its sources, the C sources of the fork that we link, and its
    flags. Every check of the probe is in the shared menu_gl_probe.cpp.
    """
    declared = native_build.recipe()["preview"].get(rmlui_paths.TARGET)
    if declared is None:
        raise SystemExit(f"the menu draw probe has no GL context for {rmlui_paths.TARGET}")
    context = declared["context"]
    return ([ROOT / name for name in context["sources"]],
            [RETROARCH / name for name in context["forkSources"]],
            [*context["flags"], *context["libraries"]])


def check_menu_draw() -> bool:
    if not LIBRARY.is_file():
        print(f"FAIL missing {LIBRARY}")
        return False
    PROBE.mkdir(parents=True, exist_ok=True)
    binary = PROBE / "menu_core_gl"
    flags = rmlui_paths.freetype("--cflags", "--libs")
    context, c_sources, platform_flags = probe_platform()
    c_objects, _ = menu_harness.compile_objects(
        [*menu_harness.FILE_LAYER, *c_sources],
        menu_harness.Toolchain("cc", "c++", ("-I", str(menu_harness.LIBRETRO_INCLUDE)), ()),
        PROBE / "file-layer")
    command = [
        "c++", "-std=c++17",
        *rmlui_paths.DEFINES,
        *[f"-I{path}" for path in HEADER_DIRS],
        f"-I{DRIVERS}",
        f"-I{menu_harness.LIBRETRO_INCLUDE}",
        f"-I{native_build.PREVIEW}",
        "-o", str(binary),
        str(NATIVE / "menu_gl_probe.cpp"),
        *map(str, context),
        str(DRIVERS / "rmlui/file_layer.cpp"),
        str(DRIVERS / "rmlui/render/rmlui_gl.cpp"),
        str(DRIVERS / "rmlui/render/rmlui_gl3.cpp"),
        str(DRIVERS / "third_party/lodepng.cpp"),
        *map(str, c_objects),
        str(LIBRARY),
        *flags,
        *platform_flags,
        *toolchain.describe()["supportLibraries"],
    ]
    compiled = subprocess.run(command, capture_output=True, text=True)
    if compiled.returncode != 0:
        print(compiled.stderr[-4000:])
        print("FAIL menu draw check did not compile")
        return False
    picture = PROBE / "João" / "slot-1.png"
    picture.parent.mkdir(parents=True, exist_ok=True)
    ran = subprocess.run(
        [str(binary), str(picture)],
        capture_output=True, text=True, timeout=60,
    )
    sys.stdout.write(ran.stdout)
    sys.stderr.write(ran.stderr)
    return ran.returncode == 0


def main() -> int:
    log_ok = check_log_line()
    draw_ok = check_menu_draw()
    if log_ok and draw_ok:
        print("dcmenu check ok")
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
