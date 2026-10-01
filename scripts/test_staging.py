"""Check that we stage the runtime kit's assets from this checkout as they are.

With `scripts/kit_assets.py` we stage every design, the shared parts, the
controller pictures, the sound packs and the branding into a kit, for every
platform. A rename can point the staging at a missing directory, and the
failure then comes on the next build, far from the change. So we stage into
a scratch kit and compare each staged file with its source. We also check
that the checkout's kit is current, and that in the builder's build tool
(scripts/build_builder.py) we look for Cargo's output where Cargo writes it.

    python3 scripts/test_staging.py
"""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import kit_assets  # noqa: E402


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
        # Files from a design, a design's document or a shared part that we
        # renamed or removed, left in a kit staged before.
        left = [kit / "designs" / "native" / "screen-retired.rml",
                kit / "designs" / "retired-design" / "design.json",
                kit / "parts" / "retired-part.rml"]
        for path in left:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("old", encoding="utf-8")
        kit_assets.stage(kit)
        wrong += [f"{path.relative_to(kit).as_posix()}: no longer in the repository, still in the kit"
                  for path in left if path.exists()]
        if (kit / "designs" / "retired-design").exists():
            wrong.append("designs/retired-design: a design the repository no longer has is still in the kit")
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


def builder_finds_cargo_output() -> list[str]:
    """Check where we look for Cargo's output in the builder's build tool, for
    each way of setting CARGO_TARGET_DIR."""
    import build_builder

    wrong: list[str] = []
    saved = os.environ.get("CARGO_TARGET_DIR")
    try:
        with tempfile.TemporaryDirectory(prefix="rominabox-target-") as shared:
            os.environ["CARGO_TARGET_DIR"] = shared
            if build_builder.cargo_output() != Path(shared).resolve():
                wrong.append("a shared CARGO_TARGET_DIR is not where the builder looks")
        os.environ["CARGO_TARGET_DIR"] = "elsewhere"
        if build_builder.cargo_output() != (build_builder.TAURI / "elsewhere").resolve():
            wrong.append("a relative CARGO_TARGET_DIR is not resolved from Cargo's working directory")
        del os.environ["CARGO_TARGET_DIR"]
        if build_builder.cargo_output() != (build_builder.TAURI / "target").resolve():
            wrong.append("with no CARGO_TARGET_DIR the builder does not look beside its manifest")
    finally:
        if saved is None:
            os.environ.pop("CARGO_TARGET_DIR", None)
        else:
            os.environ["CARGO_TARGET_DIR"] = saved
    return wrong


def builder_signs_only_mach_o() -> list[str]:
    """Check which files we sign one by one in the builder's build tool, with
    `file`, whose description of a font can contain a byte that is not UTF-8,
    for example the Latin-1 copyright sign in the Science Gothic description."""
    if sys.platform != "darwin":
        return []
    import build_builder

    wrong: list[str] = []
    font = ROOT / "integrations/designs/native/ScienceGothic-Bold.ttf"
    try:
        if build_builder.mach_o(font):
            wrong.append(f"{font.name} was taken for a Mach-O file")
    except UnicodeDecodeError as error:
        wrong.append(f"reading what {font.name} is failed: {error}")
    if not build_builder.mach_o(Path("/usr/bin/true")):
        wrong.append("/usr/bin/true was not taken for a Mach-O file")
    return wrong


def launch_library_is_current() -> list[str]:
    """We build a macOS kit's launch library when we make the kit, from the
    launcher's sources and the player headers that they include, and record
    the sources in the kit. Without that, a change to the launcher would be in
    no export until we made the kit again."""
    import json

    sys.path.insert(0, str(ROOT / "scripts"))
    import native_build
    from core_source import host_target

    target = native_build.kit_target(host_target())
    if not native_build.launch_library(target):
        return []
    recorded = KIT / "provenance/native-rmlui/source.json"
    if not recorded.is_file():
        return ["the kit records no build: make it with scripts/build_kit.py"]
    built_from = json.loads(recorded.read_text(encoding="utf-8")).get("launchLibrarySources")
    if built_from != native_build.launch_library_sources(target):
        return ["the kit's launch library was built from other launcher sources than this tree's"]
    return []


def main() -> int:
    failures: list[str] = []

    signed = builder_signs_only_mach_o()
    for entry in signed:
        print(f"  FAIL {entry}")
    failures += signed
    if not signed:
        print("  ok   the builder signs its Mach-O files, whatever `file` says of the others")

    # With CARGO_TARGET_DIR, Cargo's output may be outside this checkout. For
    # the builder we must use the same target directory as in the other build
    # scripts, for the permission pass, the CLI copy and the final app bundle.
    looked = builder_finds_cargo_output()
    for entry in looked:
        print(f"  FAIL {entry}")
    failures += looked
    if not looked:
        print("  ok   the builder looks for Cargo's output where Cargo writes it")

    wrong = staged_as_sources()
    for entry in wrong:
        print(f"  FAIL {entry}")
    failures += wrong
    if not wrong:
        print("  ok   a scratch kit is staged exactly as the sources are")

    stale = staged_designs_are_current()
    for entry in stale:
        print(f"  STALE {entry}")

    launcher = launch_library_is_current()
    for entry in launcher:
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
    if launcher:
        print(
            "\nEvery export would ship the kit's older launcher. Make the kit again:\n"
            "  python3 scripts/build_kit.py <the player build it was made from>",
        )
        return 1
    print("\nthe kit's assets stage as their sources, and this checkout's kit, its launch library too, is current")
    return 0



DESIGNS = ROOT / "integrations/designs"
PARTS = ROOT / "integrations/parts"
KIT = ROOT / "desktop/src-tauri/resources/runtime"


def staged_copy_is_current(source: Path, staged: Path, name: str) -> list[str]:
    """Return the files missing from the kit's copy of a directory, older there
    than in the source, or no longer in the source."""
    if not staged.is_dir():
        return [f"{name}: not in the kit at all"]
    stale: list[str] = []
    for document in sorted(p for p in source.iterdir() if p.is_file()):
        beside = staged / document.name
        if not beside.exists():
            stale.append(f"{name}/{document.name}: missing from the kit")
        elif beside.read_bytes() != document.read_bytes():
            stale.append(f"{name}/{document.name}: the kit's copy is older")
    for staged_file in sorted(p for p in staged.iterdir() if p.is_file()):
        if not (source / staged_file.name).is_file():
            stale.append(f"{name}/{staged_file.name}: in the kit, no longer in the repository")
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
