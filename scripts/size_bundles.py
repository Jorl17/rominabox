"""Export a few games and refuse one that is over the size ceiling.

The prepared runtime kit is ignored by Git, and we do not compile RetroArch.
The kit contains no cores. As in the builder, we take the Mega Drive core from
the local core source (scripts/core_source.py) as the export's core cache. The
ROM is a few bytes that we write in the work directory, so the size is that of
the player and the one core the game uses. A real Game Boy Advance ROM adds
its own size. We check the app on disk for this machine's platform (a macOS
bundle, a Windows game folder) against scripts/fixtures/size-budgets.json. At
export we write that app and nothing else.

We also refuse libraries the app should not contain. On macOS, the video
encoders can take a cartridge export past 50 MB, and with a budget alone we
could miss them if something else shrank to make room. On Windows we refuse
every library but the game's core, because the player is one program.

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
from core_source import core, host_target  # noqa: E402
import native_build  # noqa: E402
import prepare_runtime  # noqa: E402

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


def macos_libraries(app: Path) -> list[str]:
    """Return the problems with the libraries in a macOS app."""
    frameworks = app / "Contents/Frameworks"
    names = {path.name for path in frameworks.iterdir() if path.is_file()}
    wrong = [f"still ships {wanted}" for wanted in ABSENT if wanted in names]
    if PRESENT not in names:
        wrong.append(f"is missing {PRESENT}")
    return wrong


def windows_libraries(app: Path) -> list[str]:
    """Return the problems with the libraries in a Windows game, which are any
    library but its core. The player and the launcher use only Windows."""
    return [f"carries {path.relative_to(app)}" for path in sorted(app.rglob("*.dll"))
            if path.relative_to(app) != Path("Resources/game-core.dll")]


# For each platform, where its kit has the player (we freeze the macOS kit
# with freeze-runtime-kit.mjs, and the Windows one follows the recipe), where
# an app has its own files, and the check of the libraries in an app.
PLATFORMS = {
    "macos": {
        "player": lambda: "bin/retroarch",
        "resources": lambda app: app / "Contents" / "Resources",
        "libraries": macos_libraries,
    },
    "windows": {
        "player": lambda: native_build.recipe()["kit"][host_target()]["files"]["player"]["at"],
        "resources": lambda app: app / "Resources",
        "libraries": windows_libraries,
    },
}
PLATFORM = host_target().split("-", 1)[0]


def core_cache() -> Path:
    """Return the core cache with the Mega Drive core for these games, for
    this machine's platform."""
    artifact = prepare_runtime.catalog_components()["genesis_plus_gx"]["artifacts"][host_target()]
    return core(artifact).parent.parent


def resources(app: Path) -> Path:
    """Return the folder of an exported app's own files."""
    return PLATFORMS[PLATFORM]["resources"](app)


def export(command: Path, name: str, kit: Path, cache: Path, rom: Path, extra: dict) -> Path:
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
        "target": PLATFORM,
        "runtimeKit": str(kit),
        "coreCache": str(cache),
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
    written = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    app = next((Path(event["result"]["appPath"]) for event in written if event.get("type") == "result"), None)
    if app is None or not app.is_dir():
        raise SystemExit(f"{name} wrote no app")
    return app


def main() -> int:
    if PLATFORM not in PLATFORMS:
        raise SystemExit(f"no size check is declared for {PLATFORM}")
    platform = PLATFORMS[PLATFORM]
    player = KIT / platform["player"]()
    if not player.is_file():
        raise SystemExit(
            f"no player at {player}. This scope measures the "
            "prepared kit; it does not build one."
        )
    budgets = json.loads(BUDGETS.read_text())
    installed_ceiling = int(budgets["installed_bytes"])

    rom = WORK / "stand-in.bin"
    WORK.mkdir(parents=True, exist_ok=True)
    rom.write_bytes(b"RIBsize")
    kit = WORK / "kit"
    remove_owned(kit)
    shutil.copytree(KIT, kit, symlinks=True)
    cache = core_cache()
    for document in DESIGN.iterdir():
        if document.is_file():
            shutil.copyfile(document, kit / "designs/native" / document.name)

    command = cli()
    bundles = {
        "featured": {},
        "no-options": {"menuEntries": [], "includeAchievements": False},
        "bare": {"showMenu": False, "splash": False, "startAtMenu": False, "shaders": {}},
    }
    failed = False
    for name, extra in bundles.items():
        app = export(command, name, kit, cache, rom, extra)
        extras = sorted(
            entry.name
            for entry in app.parent.iterdir()
            if entry.name != app.name
        )
        if extras:
            print(f"  {name} wrote more than the app: {', '.join(extras)}")
            failed = True
        installed = du(app)
        print(f"  {name:<12} app {installed:8d}")
        if installed > installed_ceiling:
            print(f"  {name} app {installed} exceeds {installed_ceiling}")
            failed = True
        for wrong in platform["libraries"](app):
            print(f"  {name} {wrong}")
            failed = True
    if failed:
        return 1
    print(f"\n3 apps within {installed_ceiling} bytes on disk")
    return 0


if __name__ == "__main__":
    sys.exit(main())
