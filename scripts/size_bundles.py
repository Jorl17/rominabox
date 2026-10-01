"""Export a few games and refuse one that is over the size ceiling.

The prepared runtime kit is ignored by Git, and we do not compile RetroArch.
The kit contains no cores. As in the builder, we take the Mega Drive core from
the local core source (scripts/core_source.py) as the export's core cache. The
ROM is a few bytes that we write in the work directory, so the size is that of
the player and the one core the game uses. A real Game Boy Advance ROM adds
its own size. We check the space the game's files take on disk for this
machine's platform, in the whole blocks or clusters of the file system,
against scripts/fixtures/size-budgets.json. A game of many small files
leaves most of those blocks empty. On macOS we measure the bundle. On Windows
we measure the files unpacked from the game's one program, and then remove
the game with its own UNINSTALL. We print their bytes, and the size of the
Windows download, beside it. At export we write that app and nothing else.
On macOS we export one more game, which also runs on Intel Macs. Its player
and core contain code for both processors, it has its own ceiling, and its
Intel core comes from the macos-x86_64 folder of the core source.

We also refuse libraries the app should not contain. On macOS, the video
encoders can take a cartridge export past 50 MB, and with a budget alone we
could miss them if something else shrank to make room. On Windows we refuse
every library but the game's core, because the player is one program.

    python3 scripts/size_bundles.py
"""

from __future__ import annotations

import ctypes
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
import windows_pack  # noqa: E402
from menu_shots import QUIET_ENV, forget_windows_game  # noqa: E402

KIT = ROOT / "desktop/src-tauri/resources/runtime"
BUDGETS = ROOT / "scripts/fixtures/size-budgets.json"
WORK = ROOT / "work/size-bundles"
DESIGN = ROOT / "integrations/designs/native"


def remove_owned(path: Path) -> None:
    """Remove one directory this script created under work/size-bundles."""
    owned = (ROOT / "work" / "size-bundles").resolve()
    resolved = path.resolve()
    if resolved.parent != owned:
        raise SystemExit(f"refusing to remove {resolved}")
    if resolved.is_dir():
        shutil.rmtree(resolved)


def posix_allocated(file: Path) -> int:
    """Return the space used by `file` on a macOS or Linux volume, in blocks."""
    return file.lstat().st_blocks * 512


class FileStandardInfo(ctypes.Structure):
    """The FILE_STANDARD_INFO structure of GetFileInformationByHandleEx."""
    _fields_ = [("AllocationSize", ctypes.c_longlong), ("EndOfFile", ctypes.c_longlong),
                ("NumberOfLinks", ctypes.c_ulong), ("DeletePending", ctypes.c_ubyte),
                ("Directory", ctypes.c_ubyte)]


def windows_allocated(file: Path) -> int:
    """Return the space used by `file` on a Windows volume in NTFS, which is
    whole clusters, except for a file small enough to fit in its own record."""
    import msvcrt
    standard_info = 1  # FileStandardInfo, of FILE_INFO_BY_HANDLE_CLASS
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    info = FileStandardInfo()
    with file.open("rb") as opened:
        handle = ctypes.c_void_p(msvcrt.get_osfhandle(opened.fileno()))
        if not kernel.GetFileInformationByHandleEx(handle, standard_info, ctypes.byref(info), ctypes.sizeof(info)):
            raise ctypes.WinError(ctypes.get_last_error())
    return info.AllocationSize


def files(path: Path) -> list[Path]:
    """Every file under `path`."""
    return [Path(folder) / name for folder, _, names in os.walk(path) for name in names]


def du(path: Path) -> int:
    """Return the disk space used by everything under `path` on this platform."""
    allocated = PLATFORMS[PLATFORM]["allocated"]
    return sum(allocated(file) for file in files(path))


def apparent(path: Path) -> int:
    """The bytes of everything under `path`."""
    return sum(file.lstat().st_size for file in files(path))


def macos_libraries(app: Path) -> list[str]:
    """Return the problems with the libraries in a macOS app, which are any
    library in its Frameworks. We link only the system's libraries into the
    player and build TLS and FreeType into it (in scripts/build_player.py we
    refuse a player without TLS)."""
    frameworks = app / "Contents/Frameworks"
    return [f"carries {path.relative_to(app)}" for path in sorted(frameworks.rglob("*"))
            if path.is_file() or path.is_symlink()]


def windows_libraries(program: Path) -> list[str]:
    """Return the problems with the libraries in a Windows game, which are any
    library but its core. The player and the launcher use only Windows."""
    return [f"carries {path}" for path, _ in sorted(windows_pack.packed(program).files)
            if path.endswith(".dll") and path != "Resources/game-core.dll"]


def windows_installed(program: Path, name: str) -> tuple[int, int]:
    """Return the space used by a Windows game's unpacked files, and their
    bytes. Then remove the game's sandbox, data and unpacked copy with its
    own UNINSTALL."""
    try:
        folder = windows_pack.unpacked(program, dict(os.environ, **{QUIET_ENV: "1"}))
        return du(folder), apparent(folder)
    finally:
        forget_windows_game(program, namespace(name))


def macos_installed(app: Path, _name: str) -> tuple[int, int]:
    """Return the disk space used by a macOS app, and its bytes."""
    return du(app), apparent(app)


def windows_resources(program: Path) -> Path:
    """Return the folder of a Windows game's own files. We fill it by running
    the game's program with its silent option."""
    return windows_pack.unpacked(program, dict(os.environ, **{QUIET_ENV: "1"})) / "Resources"


# For each platform, where an app has its own files, the check of the
# libraries in an app, and the games we make only on this platform, each with
# its budget and the other target whose core we also include in it.
PLATFORMS = {
    "macos": {
        "resources": lambda app: app / "Contents" / "Resources",
        "allocated": posix_allocated,
        "installed": macos_installed,
        "libraries": macos_libraries,
        "more": {
            "featured-intel": ({"intelMacs": True}, "intel_macs_installed_bytes", "macos-x86_64"),
        },
    },
    "windows": {
        "resources": windows_resources,
        "allocated": windows_allocated,
        "installed": windows_installed,
        "libraries": windows_libraries,
        "more": {},
    },
}
PLATFORM = host_target().split("-", 1)[0]


def core_cache() -> Path:
    """Return the core cache with the Mega Drive core for these games, for
    this machine's platform."""
    artifact = prepare_runtime.catalog_components()["genesis_plus_gx"]["artifacts"][host_target()]
    return core(artifact).parent.parent


def require_core_beside(cache: Path, target: str) -> None:
    """Return the Mega Drive core for `target` in its folder beside `cache`,
    where we look for it in an export for several targets. We download nothing."""
    artifact = prepare_runtime.catalog_components()["genesis_plus_gx"]["artifacts"][target]
    beside = cache.parent / target / "cores" / artifact
    if not beside.is_file():
        raise SystemExit(
            f"{artifact} for {target} is not at {beside}. Seed it with: "
            f"python3 scripts/prepare_runtime.py --seed-core-cache --target {target}"
        )


def resources(app: Path) -> Path:
    """Return the folder of an exported app's own files."""
    return PLATFORMS[PLATFORM]["resources"](app)


def namespace(name: str) -> str:
    """Return the namespace of the games we export here, separate from a
    player's games and other checks' games (ROMINABOX_GAME_BUNDLE_PREFIX)."""
    return f"size-{name}"


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
    env = dict(os.environ, ROMINABOX_GAME_BUNDLE_PREFIX=namespace(name))
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
    if app is None or not app.exists():
        raise SystemExit(f"{name} wrote no app")
    return app


def main() -> int:
    if PLATFORM not in PLATFORMS:
        raise SystemExit(f"no size check is declared for {PLATFORM}")
    platform = PLATFORMS[PLATFORM]
    player = KIT / native_build.kit_file(host_target(), "player")
    if not player.is_file():
        raise SystemExit(
            f"no player at {player}. This scope measures the "
            "prepared kit; it does not build one."
        )
    budgets = json.loads(BUDGETS.read_text())

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
        "featured": ({}, "installed_bytes"),
        "no-options": ({"menuEntries": [], "includeAchievements": False}, "installed_bytes"),
        "bare": ({"showMenu": False, "splash": False, "startAtMenu": False, "shaders": {}}, "installed_bytes"),
    }
    for name, (extra, budget, target) in platform["more"].items():
        require_core_beside(cache, target)
        bundles[name] = (extra, budget)
    failed = False
    for name, (extra, budget) in bundles.items():
        ceiling = int(budgets[budget])
        app = export(command, name, kit, cache, rom, extra)
        extras = sorted(
            entry.name
            for entry in app.parent.iterdir()
            if entry.name != app.name
        )
        if extras:
            print(f"  {name} wrote more than the app: {', '.join(extras)}")
            failed = True
        installed, written = platform["installed"](app, name)
        download = f"  download {app.stat().st_size}" if app.is_file() else ""
        print(f"  {name:<14} on disk {installed:8d}  of {ceiling}  (bytes {written}){download}")
        if installed > ceiling:
            print(f"  {name} app {installed} exceeds {ceiling}")
            failed = True
        for wrong in platform["libraries"](app):
            print(f"  {name} {wrong}")
            failed = True
    if failed:
        return 1
    print(f"\n{len(bundles)} apps within their ceilings on disk")
    return 0


if __name__ == "__main__":
    sys.exit(main())
