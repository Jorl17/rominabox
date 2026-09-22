"""Export a few games and refuse one that is over the size ceiling.

The prepared runtime kit is ignored by Git, and we do not compile RetroArch.
The ROM is a few bytes that we write in the work directory, so the size is
that of the player and the one core the game uses. A real Game Boy Advance ROM
adds its own size. We check the installed .app and the zip a person would
download against scripts/fixtures/size-budgets.json.

We also refuse the video encoders. They can take a cartridge export past
50 MB, and with a budget alone we could miss them if something else shrank to
make room.

    python3 scripts/size_bundles.py
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
from built import cli  # noqa: E402

KIT = ROOT / "desktop/src-tauri/resources/runtime"
BUDGETS = ROOT / "scripts/fixtures/size-budgets.json"
WORK = ROOT / "work/size-bundles"
DESIGN = ROOT / "integrations/designs/native"

# OpenSSL and the FFmpeg stack, including the H.265 and AV1 encoders.
# mbedtls is not listed, because we use it for achievements and networking.
ABSENT = (
    "libavcodec.62.dylib",
    "libavfilter.11.dylib",
    "libavformat.62.dylib",
    "libx265.216.dylib",
    "libSvtAv1Enc.4.dylib",
    "libcrypto.3.dylib",
)
PRESENT = "libmbedtls.21.dylib"


def remove_owned(path: Path) -> None:
    """Remove one directory this script created under work/size-bundles."""
    owned = (ROOT / "work" / "size-bundles").resolve()
    resolved = path.resolve()
    if resolved.parent != owned:
        raise SystemExit(f"refusing to remove {resolved}")
    if resolved.is_dir():
        shutil.rmtree(resolved)


def du(path: Path) -> int:
    total = 0
    for dirpath, _, names in os.walk(path):
        for name in names:
            total += (Path(dirpath) / name).stat().st_size
    return total


def export(command: Path, name: str, kit: Path, rom: Path, extra: dict) -> Path:
    out = WORK / name
    remove_owned(out)
    out.mkdir(parents=True)
    request = {
        "rom": str(rom),
        "title": name,
        "system": "megadrive",
        "showMenu": True,
        "startAtMenu": True,
        "theme": "native",
        "palette": "blue",
        "menuSounds": "off",
        "splash": True,
        "advancedEmulatorAccess": False,
        "shaders": {"bundled": ["scanlines", "phosphor"], "initial": "phosphor"},
        "outputDir": str(out),
        "target": "macos",
        "runtimeKit": str(kit),
    }
    request.update(extra)
    env = dict(os.environ, ROMINABOX_GAME_BUNDLE_PREFIX=f"size-{name}")
    result = subprocess.run(
        [str(command), "export"],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        env=env,
    )
    if result.returncode != 0:
        raise SystemExit(f"could not export {name}:\n{result.stdout[-800:]}")
    app = next(out.glob("*.app"), None)
    if app is None:
        raise SystemExit(f"{name} wrote no .app")
    return app


def main() -> int:
    if not (KIT / "bin/retroarch").is_file():
        raise SystemExit(
            f"no player at {KIT / 'bin/retroarch'}. This scope measures the "
            "prepared kit; it does not build one."
        )
    budgets = json.loads(BUDGETS.read_text())
    installed_ceiling = int(budgets["installed_bytes"])
    archive_ceiling = int(budgets["archive_bytes"])

    rom = WORK / "stand-in.bin"
    WORK.mkdir(parents=True, exist_ok=True)
    rom.write_bytes(b"RIBsize")
    kit = WORK / "kit"
    remove_owned(kit)
    shutil.copytree(KIT, kit, symlinks=True)
    for document in DESIGN.iterdir():
        if document.is_file():
            shutil.copyfile(document, kit / "designs/native" / document.name)

    command = cli()
    bundles = {
        "featured": {},
        "no-options": {"menuEntries": []},
        "bare": {"showMenu": False, "splash": False, "startAtMenu": False, "shaders": {}},
    }
    failed = False
    for name, extra in bundles.items():
        app = export(command, name, kit, rom, extra)
        archive = next(app.parent.glob("*-macOS.zip"))
        installed = du(app)
        archived = archive.stat().st_size
        frameworks = app / "Contents/Frameworks"
        names = {path.name for path in frameworks.iterdir() if path.is_file()}
        print(
            f"  {name:<12} installed {installed:8d}  archive {archived:8d}  "
            f"frameworks {len(names)}"
        )
        if installed > installed_ceiling:
            print(f"  {name} installed {installed} exceeds {installed_ceiling}")
            failed = True
        if archived > archive_ceiling:
            print(f"  {name} archive {archived} exceeds {archive_ceiling}")
            failed = True
        carried = [wanted for wanted in ABSENT if wanted in names]
        if carried:
            print(f"  {name} still ships {', '.join(carried)}")
            failed = True
        if PRESENT not in names:
            print(f"  {name} is missing {PRESENT}")
            failed = True
    if failed:
        return 1
    print(
        f"\n3 bundles within {installed_ceiling} installed bytes "
        f"and {archive_ceiling} archive bytes"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
