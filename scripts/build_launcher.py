"""Build the game's launcher for this machine into a new folder, and print
where it is.

    uv run python scripts/build_launcher.py /absolute/new/folder
    uv run python scripts/build_launcher.py --kit KIT    # macOS: into KIT, attached to its player

We build a Windows kit's launcher beside the player in scripts/build_player.py
and a macOS kit's launch library in scripts/build_kit.py. For tests that
require the launcher but not a whole player or kit, we build it here with the
same recipe and functions, in this checkout's folder for that build
(native_build.tree_build), and give the tests a copy.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
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
    kit = native_build.kit_target(host_target())
    files = native_build.recipe()["kit"][kit]["files"]
    print(native_build.tree_launcher(kit, destination / Path(files["launcher"]["at"]).name))
    return 0


def install_in_kit(kit_folder: Path) -> int:
    """Build a macOS kit's launch library into the kit at `kit_folder` and
    attach it to the player, as we do for a kit in scripts/build_kit.py."""
    kit = native_build.kit_target(host_target())
    if not native_build.launch_library(kit):
        raise SystemExit(f"a {kit} kit's launcher is built with its player, not into a kit")
    native_build.install_tree_launcher(kit_folder.resolve(), kit)
    # In the kit we record the sources we built its launch library from
    # (scripts/build_kit.py), and this build is now that library.
    recorded = kit_folder / "provenance" / "native-rmlui" / "source.json"
    if recorded.is_file():
        source = json.loads(recorded.read_text(encoding="utf-8"))
        source["launchLibrarySources"] = native_build.launch_library_sources(kit)
        recorded.write_text(json.dumps(source, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(kit_folder / native_build.kit_file(host_target(), "launcher"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
