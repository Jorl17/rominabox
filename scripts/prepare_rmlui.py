"""Build the RmlUi that the tests link, with the commands of the player build.

A fresh clone has no compiled RmlUi. bridge and dcmenu then stop on a
library that was never produced, and menu cannot compile its probe. The
clone, the commit and the cmake flags are only in the player build script,
and we run those lines from it. With a second copy of the flags, the tests
could link a different RmlUi from the one in the player.
"""

from __future__ import annotations

import fcntl
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import free_space  # noqa: E402
from rmlui_paths import (  # noqa: E402
    BUILD_DIR,
    DEST,
    HEADER,
    LIBRARY,
    PLAYER_BUILD,
    SOURCE,
)

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "work" / "rmlui-prepare.lock"
STAMP = DEST / "recipe.txt"


def player_recipe() -> str:
    """The clone, checkout and cmake lines, unchanged, from the player build.

    The script also copies a makefile and configures RetroArch between those
    lines. If we took the whole span, that copy would run with a variable that
    we do not set here, and the RmlUi build would not start.
    """
    lines = PLAYER_BUILD.read_text(encoding="utf-8").splitlines()
    kept: list[str] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        if "git clone" in line and "RmlUi.git" in line:
            kept.append(line)
        elif "git -C" in line and "RmlUi" in line and "checkout" in line:
            kept.append(line)
        elif line.startswith("cmake -S") and "RmlUi" in line:
            kept.append(line)
            while line.rstrip().endswith("\\"):
                index += 1
                line = lines[index]
                kept.append(line)
        elif line.strip().startswith("cmake --build") and "$destination" in line:
            kept.append(line)
        index += 1
    if not any("git clone" in line for line in kept) or not any(
        line.strip().startswith("cmake --build") for line in kept
    ):
        raise SystemExit(
            f"{PLAYER_BUILD.name} no longer contains the RmlUi clone and cmake "
            "build the tests run"
        )
    return "\n".join(kept) + "\n"


def ready(recipe: str) -> bool:
    return (
        STAMP.is_file()
        and STAMP.read_text(encoding="utf-8") == recipe
        and LIBRARY.is_file()
        and HEADER.is_file()
    )


def commands(recipe: str, source_present: bool) -> str:
    kept: list[str] = []
    for line in recipe.splitlines():
        if line.strip().startswith("git clone") and source_present:
            continue
        kept.append(line)
    return "\n".join(kept) + "\n"


def produce() -> None:
    recipe = player_recipe()
    if ready(recipe):
        return
    free_space.require(20, DEST)
    DEST.mkdir(parents=True, exist_ok=True)
    SOURCE.parent.mkdir(parents=True, exist_ok=True)
    jobs = str(os.cpu_count() or 1)
    environment = {**os.environ, "destination": str(DEST), "jobs": jobs}
    print(f"building RmlUi into {DEST}", flush=True)
    subprocess.run(
        ["sh", "-eu", "-c", commands(recipe, (SOURCE / ".git").exists())],
        check=True,
        env=environment,
    )
    if not LIBRARY.is_file() or not HEADER.is_file():
        raise SystemExit(
            f"the player recipe ran, but {LIBRARY} or {HEADER} is not there"
        )
    STAMP.write_text(recipe, encoding="utf-8")
    # We parse the build directory of the declaration from the same -B flag
    # that these lines just used. If cmake wrote somewhere else, the tests
    # would look at an empty path and report a missing library that was built.
    if not BUILD_DIR.is_dir():
        raise SystemExit(f"the player recipe did not create {BUILD_DIR}")


def main() -> int:
    # We prepare bridge, dcmenu and menu together. Without the lock, three
    # clones write one directory and each reports the failure of another.
    LOCK.parent.mkdir(parents=True, exist_ok=True)
    with LOCK.open("a") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        produce()
    return 0


if __name__ == "__main__":
    sys.exit(main())
