"""Check that we stage the runtime kit's assets from this checkout as they are.

With `scripts/kit_assets.py` we stage every design, the shared parts, the
controller pictures, the sound packs and the branding into a kit, for every
platform. A rename can point the staging at a missing directory, and the
failure then comes on the next build, far from the change. So we stage into
a scratch kit and compare each staged file with its source. We also check
that the checkout's kit is current, and that the macOS builder script uses
the shared Cargo target.

    python3 scripts/test_staging.py
"""

from __future__ import annotations

import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import kit_assets  # noqa: E402

SCRIPT = ROOT / "scripts/native_runtime/build-builder-macos.sh"


def same_files(source: Path, staged: Path, names: list[str] | None = None) -> list[str]:
    """Return the files in `source` (or only `names`) missing or different in `staged`."""
    names = names if names is not None else sorted(p.name for p in source.iterdir() if p.is_file())
    wrong = []
    for name in names:
        copy = staged / name
        if not copy.is_file():
            wrong.append(f"{staged.name}/{name}: not staged")
        elif copy.read_bytes() != (source / name).read_bytes():
            wrong.append(f"{staged.name}/{name}: differs from its source")
    return wrong


def staged_as_sources() -> list[str]:
    """Stage into a scratch kit, then compare it with the sources."""
    wrong: list[str] = []
    with tempfile.TemporaryDirectory(prefix="rominabox-staging-") as temporary:
        kit = Path(temporary) / "runtime"
        retired = kit / "sound-packs" / "retired-pack"
        retired.mkdir(parents=True)
        (retired / "ok.wav").write_bytes(b"old")
        kit_assets.stage(kit)
        for design in sorted(p for p in kit_assets.DESIGNS.iterdir() if p.is_dir()):
            wrong += same_files(design, kit / "designs" / design.name)
        wrong += same_files(kit_assets.PARTS, kit / "parts")
        wrong += same_files(kit_assets.CONTROLLERS, kit / "menu-assets",
                            [*kit_assets.controller_assets(), "CONTROLLERS.txt"])
        packs = sorted(p for p in kit_assets.SOUNDS.iterdir() if p.is_dir())
        for pack in packs:
            wrong += same_files(pack, kit / "sound-packs" / pack.name, [f"{cue}.wav" for cue in kit_assets.CUES])
        if retired.exists():
            wrong.append("sound-packs/retired-pack: a pack the repository no longer ships is still in the kit")
        wrong += same_files(kit_assets.BRANDING, kit / "branding", ["logo.png", "PROVENANCE.txt"])
        wrong += same_files(kit_assets.DEFAULT_ICON.parent, kit, [kit_assets.DEFAULT_ICON.name])
        if not packs or not (kit / "designs" / "native").is_dir():
            wrong.append("staging produced no native design or no sound pack; this check proves nothing")
    return wrong


def builder_uses_cargo_target(text: str) -> bool:
    return "from built import target_dir" in text and all(
        path in text
        for path in (
            'staging="$cargo_target_dir/$profile"',
            'cp "$cargo_target_dir/release/rominabox-cli" resources/bin/rominabox-cli',
            'app="$cargo_target_dir/release/bundle/macos/ROM-in-a-Box.app"',
        )
    )


def main() -> int:
    failures: list[str] = []

    # Cargo may write into the shared target outside this checkout. For the
    # builder we must use the same target directory as in the other build
    # scripts, for the permission pass, the CLI copy and the final app bundle.
    if not builder_uses_cargo_target(SCRIPT.read_text()):
        failures.append("builder does not use built.target_dir for every Cargo output")

    wrong = staged_as_sources()
    for entry in wrong:
        print(f"  FAIL {entry}")
    failures += wrong
    if not wrong:
        print("  ok   a scratch kit is staged exactly as the sources are")

    stale = staged_designs_are_current()
    for entry in stale:
        print(f"  STALE {entry}")

    if failures:
        print(f"\n{len(failures)} staging problem(s)")
        return 1
    if stale:
        print(
            f"\n{len(stale)} staged design file(s) are older than the design they "
            "came from, so the builder and an exported game draw something the "
            "tests never render. Restage:\n"
            "  python3 scripts/kit_assets.py",
        )
        return 1
    print("\nthe kit's assets stage as their sources, and this checkout's kit is current")
    return 0



DESIGNS = ROOT / "integrations/designs"
PARTS = ROOT / "integrations/parts"
KIT = ROOT / "desktop/src-tauri/resources/runtime"


def staged_copy_is_current(source: Path, staged: Path, name: str) -> list[str]:
    """Return the files missing from the kit's copy of a directory, or older there."""
    if not staged.is_dir():
        return [f"{name}: not in the kit at all"]
    stale: list[str] = []
    for document in sorted(p for p in source.iterdir() if p.is_file()):
        beside = staged / document.name
        if not beside.exists():
            stale.append(f"{name}/{document.name}: missing from the kit")
        elif beside.read_bytes() != document.read_bytes():
            stale.append(f"{name}/{document.name}: the kit's copy is older")
    return stale


def staged_designs_are_current() -> list[str]:
    """Check that the kit's copy of each design, and of the shared parts of every
    design, matches what we staged it from.

    The kit is build output and not a second copy that we maintain by hand, so
    this checks a cache for staleness. A kit staged before a design changed
    can contain an older document, for example an older frame, while every
    test stages from the source.
    """
    stale: list[str] = []
    for design in sorted(p for p in DESIGNS.iterdir() if p.is_dir()):
        stale += staged_copy_is_current(design, KIT / "designs" / design.name, design.name)
    stale += staged_copy_is_current(PARTS, KIT / "parts", "parts")
    return stale


if __name__ == "__main__":
    sys.exit(main())
