"""Stage the runtime kit's own assets from this checkout, for every platform.

    python3 scripts/kit_assets.py [KIT]      # default: desktop/src-tauri/resources/runtime

What we read from the kit on export besides the player: every menu design,
the shared parts we compose them with, the controller pictures, the menu
sound packs and the branding. We copy them from the repository unchanged,
so this is the same on macOS and Windows. We add the player, its libraries
and the licences for each platform in the kit tool.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KIT = ROOT / "desktop/src-tauri/resources/runtime"
DESIGNS = ROOT / "integrations/designs"
PARTS = ROOT / "integrations/parts"
CONTROLLERS = ROOT / "desktop/assets/controllers"
SOUNDS = ROOT / "desktop/assets/menu-sounds"
BRANDING = ROOT / "desktop/assets/branding"
DEFAULT_ICON = ROOT / "desktop/assets/default-icon.png"
# A pack is one complete set of cues.
CUES = ("up", "down", "ok", "cancel")


def controller_assets() -> list[str]:
    """The controller pictures declared in the catalog, so nobody who adds a
    console has to remember to extend a list here."""
    listed = subprocess.run(
        ["cargo", "run", "--quiet", "--manifest-path", str(ROOT / "desktop/crates/rominabox-catalog/Cargo.toml"),
         "--bin", "rominabox-catalog", "--", "assets"],
        capture_output=True, text=True,
    )
    if listed.returncode != 0:
        raise SystemExit(f"could not ask the catalog which controller assets to stage\n{listed.stderr}")
    return listed.stdout.split()


def copy_tree(source: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, destination, dirs_exist_ok=True)


def stage(kit: Path) -> None:
    # A worktree shares the prepared kit with the checkout it was made from, by
    # symlink, so we do not rebuild the player in each worktree. Staging
    # through the link would change the other checkout's kit and the kit of
    # every worktree linked to it.
    if kit.is_symlink():
        raise SystemExit(
            f"This checkout shares its prepared runtime kit with another one:\n\n"
            f"  {kit} -> {os.readlink(kit)}\n\n"
            "Staging into it would change that checkout's kit, and every worktree linked to\n"
            "it. If this worktree needs its own kit, which it does if it is changing a\n"
            "design or the fork, ask for one:\n\n"
            "  python3 scripts/worktree.py create <name> --own-runtime\n"
        )
    kit.mkdir(parents=True, exist_ok=True)

    # One directory per design, so the kit contains the requested designs
    # rather than one nameless set of menu files. A design package is a
    # directory, so we stage the whole directory instead of a list of its
    # documents that could disagree with the design.
    designs = [design for design in sorted(DESIGNS.iterdir()) if design.is_dir()]
    if not any(design.name == "native" for design in designs):
        raise SystemExit(f"Missing design package: {DESIGNS / 'native'}")
    for design in designs:
        copy_tree(design, kit / "designs" / design.name)

    # When we compose a menu, we link and fill in the shared parts of every
    # design, and look for them beside the designs directory: <kit>/parts next
    # to <kit>/designs. Without them we cannot compose any menu from the kit.
    if not PARTS.is_dir():
        raise SystemExit(f"Missing shared menu parts: {PARTS}")
    copy_tree(PARTS, kit / "parts")

    # Controller artwork is not part of a design. Every design shows the same
    # pads, so in the exporter we read them from the kit's shared directory.
    assets = controller_assets()
    for name in [*assets, "CONTROLLERS.txt"]:
        if not (CONTROLLERS / name).is_file():
            raise SystemExit(f"Missing controller source: {name}")
    shared = kit / "menu-assets"
    shared.mkdir(parents=True, exist_ok=True)
    for name in [*assets, "CONTROLLERS.txt"]:
        shutil.copy2(CONTROLLERS / name, shared / name)

    # In the exporter we stage the selected menu sound pack from the kit, so the
    # kit has exactly the packs in the repository. We remove a retired pack so
    # that nobody can select or bundle it.
    if not (SOUNDS / "PROVENANCE.txt").is_file():
        raise SystemExit("Missing menu sounds; run node scripts/native_runtime/generate-menu-sounds.mjs")
    staging = kit / "sound-packs"
    staging.mkdir(parents=True, exist_ok=True)
    packs = {pack.name for pack in SOUNDS.iterdir() if pack.is_dir()}
    for staged in sorted(staging.iterdir()):
        # We only ever remove a folder, not a link, directly in the kit's sound-packs.
        if (staged.parent == staging and staged.is_dir() and not staged.is_symlink()
                and staged.name and staged.name not in packs):
            shutil.rmtree(staged)
    for pack in sorted(packs):
        (staging / pack).mkdir(exist_ok=True)
        for cue in CUES:
            source = SOUNDS / pack / f"{cue}.wav"
            if not source.is_file():
                raise SystemExit(f"Menu sound pack {pack} is missing {cue}.wav")
            shutil.copy2(source, staging / pack / f"{cue}.wav")
    shutil.copy2(SOUNDS / "PROVENANCE.txt", staging / "PROVENANCE.txt")

    (kit / "branding").mkdir(exist_ok=True)
    shutil.copy2(BRANDING / "logo.png", kit / "branding" / "logo.png")
    shutil.copy2(BRANDING / "PROVENANCE.txt", kit / "branding" / "PROVENANCE.txt")
    shutil.copy2(DEFAULT_ICON, kit / "default-icon.png")


def main() -> int:
    if len(sys.argv) > 2:
        raise SystemExit("usage: kit_assets.py [KIT]")
    stage(Path(sys.argv[1]).resolve() if len(sys.argv) == 2 else KIT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
