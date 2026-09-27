"""Build the game's launcher for this machine into a new folder, and print
where it is.

    python3 scripts/build_launcher.py /absolute/new/folder

We build a kit's launcher beside the player in scripts/build_player.py. For
tests that require the launcher but not a whole player, we build it here,
from the same recipe with the same function. On macOS we build the launcher
at export, so there is nothing to build here, and we print that.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
import toolchain  # noqa: E402
from core_source import host_target  # noqa: E402


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    destination = Path(sys.argv[1])
    if not destination.is_absolute():
        raise SystemExit("the folder must be an absolute path")
    destination.mkdir(parents=True, exist_ok=False)
    target = host_target()
    toolchain.activate()
    built = native_build.build_launcher(destination, target, dict(os.environ))
    if built is None:
        raise SystemExit(f"{target} builds its launcher at export; there is none to build here")
    print(built)
    return 0


if __name__ == "__main__":
    sys.exit(main())
