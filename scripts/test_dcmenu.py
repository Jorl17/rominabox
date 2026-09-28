"""Check the menu drawing in an OpenGL core context, and log lines in a file.

The Flycast core runs in an OpenGL core context, where nothing appears when
we draw with client arrays, as RmlUi's GL2 backend does. A Dreamcast game
would then have no pause menu. We must still draw in a legacy context too.

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
LAUNCHER = ROOT / "desktop/src-tauri/launcher/macos/main.c"
LINE = PROBE / "line.txt"


def extract_line_buffer() -> str:
    source = LAUNCHER.read_text()
    signature = "void rominabox_line_buffer_stdio(void)"
    start = source.find(signature)
    if start < 0:
        raise SystemExit("launcher has no rominabox_line_buffer_stdio")
    brace = source.find("{", start)
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start:index + 1]
    raise SystemExit("rominabox_line_buffer_stdio has no closing brace")


def check_log_line() -> bool:
    if native_build.is_windows(rmlui_paths.TARGET):
        # We fork and redirect output in the same way as the macOS launcher,
        # in one process. On Windows the player runs in a separate process
        # with a log handle from its launcher, and we flush each line in
        # RetroArch's logger. We do not test the Windows case here.
        print("not on Windows: this checks the macOS launcher's in-process log redirection")
        return True
    text = LAUNCHER.read_text()
    dup = text.find("dup2(log_fd, STDOUT_FILENO)")
    call = text.find("rominabox_line_buffer_stdio()", dup if dup >= 0 else 0)
    if dup < 0 or call < 0 or call > dup + 400:
        print("FAIL launcher does not line-buffer launch.log")
        return False

    PROBE.mkdir(parents=True, exist_ok=True)
    if LINE.exists():
        LINE.unlink()
    program = r"""
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

__BODY__

int main(int argc, char **argv) {
    if (argc == 3 && strcmp(argv[1], "child") == 0) {
        int fd = open(argv[2], O_WRONLY | O_CREAT | O_TRUNC, 0644);
        if (fd < 0)
            return 2;
        if (dup2(fd, STDOUT_FILENO) < 0 || dup2(fd, STDERR_FILENO) < 0)
            return 2;
        if (fd > STDERR_FILENO)
            close(fd);
        rominabox_line_buffer_stdio();
        printf("menu line\n");
        _exit(0);
    }
    pid_t child = fork();
    if (child < 0)
        return 2;
    if (child == 0) {
        execl(argv[0], argv[0], "child", argv[1], (char *)NULL);
        _exit(2);
    }
    int status = 1;
    waitpid(child, &status, 0);
    FILE *file = fopen(argv[1], "r");
    char body[64] = {0};
    if (file) {
        if (!fgets(body, sizeof body, file))
            body[0] = '\0';
        fclose(file);
    }
    if (strstr(body, "menu line") == NULL) {
        printf("FAIL a log line never reached the file\n");
        return 1;
    }
    printf("ok a log line reached the file before exit\n");
    return 0;
}
""".replace("__BODY__", extract_line_buffer())
    source = PROBE / "log_lines.c"
    binary = PROBE / "log_lines"
    source.write_text(program)
    compiled = subprocess.run(
        ["cc", "-o", str(binary), str(source)],
        capture_output=True, text=True,
    )
    if compiled.returncode != 0:
        print(compiled.stderr)
        print("FAIL log line check did not compile")
        return False
    ran = subprocess.run(
        [str(binary), str(LINE)],
        capture_output=True, text=True, timeout=20,
    )
    sys.stdout.write(ran.stdout)
    sys.stderr.write(ran.stderr)
    if LINE.exists():
        LINE.unlink()
    return ran.returncode == 0


def probe_platform() -> tuple[list[Path], list[Path], list[str]]:
    """Return the OpenGL context in which we draw in the probe, which is the
    renderer draws in, as the recipe declares it for this target. Its
    sources, the fork's C sources it links, and its flags. Everything the
    probe checks is menu_gl_probe.cpp, shared.
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
