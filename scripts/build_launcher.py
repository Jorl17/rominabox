"""Build the game's launcher for this machine into a new folder, and print
where it is.

    python3 scripts/build_launcher.py /absolute/new/folder
    python3 scripts/build_launcher.py --kit KIT    # macOS: into KIT, attached to its player

We build a Windows kit's launcher beside the player in scripts/build_player.py
and a macOS kit's launch library in scripts/build_kit.py. For tests that
require the launcher but not a whole player or kit, we build it here with the
same recipe and functions.
"""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
import toolchain  # noqa: E402
from core_source import host_target  # noqa: E402


def main() -> int:
    if len(sys.argv) == 3 and sys.argv[1] == "--kit":
        return install_in_kit(Path(sys.argv[2]))
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    destination = Path(sys.argv[1])
    if not destination.is_absolute():
        raise SystemExit("the folder must be an absolute path")
    destination.mkdir(parents=True, exist_ok=False)
    target = host_target()
    toolchain.activate()
    kit = native_build.kit_target(target)
    if native_build.launch_library(kit):
        built = native_build.build_launch_library(destination, kit)
    else:
        built = native_build.build_launcher(destination, target, dict(os.environ))
    if built is None:
        raise SystemExit(f"the player recipe builds no launcher for {target}")
    print(built)
    return 0


def install_in_kit(kit_folder: Path) -> int:
    """Build a macOS kit's launch library into the kit at `kit_folder` and
    attach it to the player, as we do for a kit in scripts/build_kit.py."""
    kit = native_build.kit_target(host_target())
    if not native_build.launch_library(kit):
        raise SystemExit(f"a {kit} kit's launcher is built with its player, not into a kit")
    with tempfile.TemporaryDirectory(prefix="rominabox-launcher-") as workspace:
        native_build.install_launch_library(kit_folder.resolve(), kit, Path(workspace))
    print(kit_folder / native_build.kit_file(host_target(), "launcher"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
