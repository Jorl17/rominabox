"""The location of the RmlUi from the player build, and of the FreeType linked
into it, for the tests.

We read the clone and build directories from the player recipe, and the
archive linked into the player and its header directories from the player's
makefile, so this file cannot describe a second layout.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import core_source  # noqa: E402
import native_build  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# We fill this with prepare_rmlui.py, never by hand.
DEST = ROOT / "work" / "rmlui"
TARGET = core_source.host_target()

_MAKEFILE = ROOT / "vendor/retroarch/Makefile.common"


def _archive_name() -> str:
    if not _MAKEFILE.is_file():
        raise SystemExit(f"missing {_MAKEFILE}")
    makefile = _MAKEFILE.read_text(encoding="utf-8", errors="replace")
    found = re.search(r"\$\(RMLUI_BUILD_DIR\)/(\S+)", makefile)
    if not found:
        raise SystemExit("Makefile.common no longer links an RmlUi archive")
    return found.group(1)


def _header_dirs(source: Path) -> list[Path]:
    if not _MAKEFILE.is_file():
        raise SystemExit(f"missing {_MAKEFILE}")
    makefile = _MAKEFILE.read_text(encoding="utf-8", errors="replace")
    found: list[Path] = []
    for name in re.findall(r"\$\(RMLUI_SOURCE_DIR\)/(\S+)", makefile):
        path = source / name
        if path not in found:
            found.append(path)
    if not found:
        raise SystemExit("Makefile.common no longer names RmlUi header directories")
    return found


def _defines() -> list[str]:
    """Return the RmlUi defines of the player build, such as the static-library one.

    Without RMLUI_STATIC_LIB on Windows, every function in the RmlUi headers
    is declared as a DLL import, and we cannot link a probe with the archive.
    """
    makefile = _MAKEFILE.read_text(encoding="utf-8", errors="replace")
    return list(dict.fromkeys(re.findall(r"DEFINES \+= (-DRMLUI_\w+)", makefile)))


SOURCE = DEST / native_build.recipe()["rmlui"]["source"]
BUILD_DIR = DEST / native_build.recipe()["rmlui"]["build"]
LIBRARY = BUILD_DIR / _archive_name()
HEADER_DIRS = _header_dirs(SOURCE)
DEFINES = _defines()


def _include_dir() -> Path:
    """Return the include directory for `#include <RmlUi/Core.h>`.

    We find it in the list in the player makefile by the directory's name, so
    that list stays the only copy of the layout.
    """
    for path in HEADER_DIRS:
        if path.name == "Include":
            return path
    raise SystemExit(
        "Makefile.common no longer compiles against RmlUi's Include directory"
    )


INCLUDE = _include_dir()
HEADER = INCLUDE / "RmlUi" / "Core.h"


def freetype(*flags: str) -> list[str]:
    """Return pkg-config's `flags` for the FreeType linked into the player here.

    For a target with its own FreeType build, we install it beside RmlUi. With
    the system's pkg-config, we would link the tests against another one.
    """
    environment = {**native_build.build_environment(TARGET),
                   **native_build.freetype_environment(DEST, TARGET)}
    return subprocess.run([native_build.resolve("pkg-config", environment), *flags, "freetype2"],
                          capture_output=True, text=True, check=True, env=environment).stdout.split()


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] not in {"library", "includes"}:
        raise SystemExit("usage: rmlui_paths.py library|includes")
    if sys.argv[1] == "library":
        print(LIBRARY)
        return
    print(" ".join(f"-I{path}" for path in HEADER_DIRS))


if __name__ == "__main__":
    main()
