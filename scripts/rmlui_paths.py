"""The location of the RmlUi from the player build, for the tests.

We read the clone path, the cmake build directory and the archive linked into
the player from the player build, so this file cannot describe a second
layout.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# We fill this with prepare_rmlui.py, never by hand.
DEST = ROOT / "work" / "rmlui"

PLAYER_BUILD = ROOT / "scripts/native_runtime/build-retroarch-rmlui-macos.sh"
_MAKEFILE = ROOT / "vendor/retroarch/Makefile.common"


def _player_text() -> str:
    if not PLAYER_BUILD.is_file():
        raise SystemExit(f"missing player build script at {PLAYER_BUILD}")
    return PLAYER_BUILD.read_text(encoding="utf-8")


def _relative_dirs() -> tuple[str, str]:
    script = _player_text()
    source = re.search(r'git clone \S+ "\$destination/([^"]+)"', script)
    build = re.search(r'-B "\$destination/([^"]+)"', script)
    if not source or not build:
        raise SystemExit(
            "the player build no longer says where it clones or builds RmlUi"
        )
    return source.group(1), build.group(1)


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


_SOURCE_REL, _BUILD_REL = _relative_dirs()
SOURCE = DEST / _SOURCE_REL
BUILD_DIR = DEST / _BUILD_REL
LIBRARY = BUILD_DIR / _archive_name()
HEADER_DIRS = _header_dirs(SOURCE)


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


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] not in {"library", "includes"}:
        raise SystemExit("usage: rmlui_paths.py library|includes")
    if sys.argv[1] == "library":
        print(LIBRARY)
        return
    print(" ".join(f"-I{path}" for path in HEADER_DIRS))


if __name__ == "__main__":
    main()
