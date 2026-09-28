"""Build the RmlUi that the tests link, with the steps of the player build.

A fresh clone has no compiled RmlUi. bridge, dcmenu and menu then stop on a
library that was never produced, and menu cannot compile its probe. We
build RmlUi and FreeType from the player recipe with the same function as
the player build, so the tests cannot link a different RmlUi from the one
in the player.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import file_lock  # noqa: E402
import free_space  # noqa: E402
import native_build  # noqa: E402
from rmlui_paths import DEST, HEADER, LIBRARY, TARGET  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "work" / "rmlui-prepare.lock"
STAMP = DEST / "recipe.txt"


def recipe() -> str:
    """The parts of the player recipe this build depends on, for this target."""
    whole = native_build.recipe()
    return json.dumps({"rmlui": whole["rmlui"], "freetype": whole["freetype"],
                       "cmake": native_build.cmake_flags(TARGET)},
                      indent=1, sort_keys=True) + "\n"


def ready(recipe: str) -> bool:
    return (
        STAMP.is_file()
        and STAMP.read_text(encoding="utf-8") == recipe
        and LIBRARY.is_file()
        and HEADER.is_file()
    )


def produce() -> None:
    wanted = recipe()
    if ready(wanted):
        return
    free_space.require(20, DEST)
    DEST.mkdir(parents=True, exist_ok=True)
    print(f"building RmlUi into {DEST}", flush=True)
    native_build.build_rmlui(DEST, TARGET, os.cpu_count() or 1)
    if not LIBRARY.is_file() or not HEADER.is_file():
        raise SystemExit(
            f"the player recipe ran, but {LIBRARY} or {HEADER} is not there"
        )
    STAMP.write_text(wanted, encoding="utf-8")


def main() -> int:
    # We prepare bridge, dcmenu and menu together. Without the lock, three
    # clones write one directory and each reports the failure of another.
    LOCK.parent.mkdir(parents=True, exist_ok=True)
    with LOCK.open("a") as handle:
        file_lock.hold_exclusively(handle)
        produce()
    return 0


if __name__ == "__main__":
    sys.exit(main())
