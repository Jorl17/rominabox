"""Check that quitting unloads the core before the process exits.

Cmd-Q, the menu bar Quit, closing the last window and an Apple Event quit
all lead to applicationShouldTerminate. If we return NSTerminateNow, exit()
runs on that stack, and the Flycast static destructors then abort while
its threads are still running. From the pause menu we request
CMD_EVENT_QUIT, and in the draw observer we run main_exit before exit. We
launch an exported game under lldb and send the Apple Event, so that a
regression shows up as the abort and not as a changed string.
"""

from __future__ import annotations

import os
import subprocess
import sys
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import menu_shots  # noqa: E402

EXPORT_DIR = ROOT / "work/quit-export"
APP = EXPORT_DIR / "Quit Subject.app"
STUB_DIR = ROOT / "work/quit-stub"
DISC = Path("/Users/mariowilde/Downloads/roms/Sonic Adventure 2 (Europe)")
GDI = "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es).gdi"
TRACKS = (
    "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es) (Track 1).bin",
    "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es) (Track 2).bin",
    "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es) (Track 3).bin",
)
PREFIX = "app.rominabox.game.wt-quit"


def free_gb() -> float:
    line = subprocess.run(
        ["df", "-k", "/Users/mariowilde"], capture_output=True, text=True, check=True
    ).stdout.splitlines()[1]
    return int(line.split()[3]) / 1024 / 1024


def require_disk(floor: float) -> None:
    free = free_gb()
    if free < floor:
        raise SystemExit(f"disk has {free:.2f} GB free, below {floor:.0f}; stopping")


def launched_player() -> Path:
    """Return the binary for the quit tests.

    menu_shots.built_player contains the path of the checkout's build. With
    no fresh build, we keep in the export the retroarch copied from the
    runtime kit, which is the player in a packaged game.
    """
    found = menu_shots.built_player()
    if found is not None:
        return found
    return menu_shots.KIT / "bin/retroarch"


def use_checkout_player(app: Path, workspace: Path) -> None:
    retroarch = app / "Contents/MacOS/retroarch"
    if not retroarch.is_file():
        raise SystemExit(f"exported game has no player at {retroarch}")
    # For the pause row we edit launch.plan, and to sign again we need the
    # entitlements from the export. After a replacement there is nothing to read.
    binary = menu_shots.built_player()
    if binary is None:
        menu_shots.capture_export_entitlements(app, workspace / "entitlements.plist")
        return
    install_player(app, binary, workspace)


def discard_owned(literal: str, app_name: str) -> None:
    """Remove the previous export of the quit tests, and nothing else.

    We check the directory twice: it must resolve to the literal path, and
    every name in it must be one that we write in these tests. An old
    Quit Subject.app may contain a launcher without the switch.
    """
    path = Path(literal)
    if not path.exists():
        return
    if path.resolve() != Path(literal) or not path.is_dir():
        raise SystemExit(f"{literal} is not this scope's directory")
    allowed = {app_name, "entitlements.plist", "inject-dylib", "unused-shot.png"}
    if app_name == "Quit Subject.app":
        allowed.add("Quit Subject-macOS.zip")
    stray = [child.name for child in path.iterdir() if child.name not in allowed]
    if stray:
        raise SystemExit(f"{literal} holds {stray}, so it is not this scope's directory")
    marker = path / app_name
    if marker.exists() and not marker.is_dir():
        raise SystemExit(f"{literal} is not this scope's directory")
    if path.resolve() != Path(literal) or not path.is_dir():
        raise SystemExit(f"{literal} changed before removal")
    if literal == "/Users/mariowilde/src/rominabox-quiet/work/quit-export":
        subprocess.run(
            ["rm", "-rf", "/Users/mariowilde/src/rominabox-quiet/work/quit-export"],
            check=True, timeout=60,
        )
    elif literal == "/Users/mariowilde/src/rominabox-quiet/work/quit-gba":
        subprocess.run(
            ["rm", "-rf", "/Users/mariowilde/src/rominabox-quiet/work/quit-gba"],
            check=True, timeout=60,
        )
    elif literal == "/Users/mariowilde/src/rominabox-quiet/work/quit-ape":
        subprocess.run(
            ["rm", "-rf", "/Users/mariowilde/src/rominabox-quiet/work/quit-ape"],
            check=True, timeout=60,
        )
    else:
        raise SystemExit(f"refusing to remove {literal}")


def export_stub() -> None:
    """Export a tiny sheet, and clone the disc into the app afterwards.

    A copy in Rust is a clone on APFS, so the disc does not have to be in the
    export that we build first.
    """
    require_disk(20.3)
    STUB_DIR.mkdir(parents=True, exist_ok=True)
    stub_bin = STUB_DIR / "stub.bin"
    stub_gdi = STUB_DIR / "stub.gdi"
    stub_bin.write_bytes(b"\0" * 2048)
    stub_gdi.write_text("1\n1 0 4 2352 stub.bin 0\n")
    # For a named run we export again, into the directory of these tests, so
    # that the app has a launcher with the switch.
    discard_owned(
        "/Users/mariowilde/src/rominabox-quiet/work/quit-export",
        "Quit Subject.app",
    )
    EXPORT_DIR.mkdir(parents=True)
    from built import cli  # noqa: E402

    request = {
        "rom": str(stub_gdi),
        "title": "Quit Subject",
        "system": "dreamcast",
        "showMenu": True,
        "startAtMenu": False,
        "theme": "native",
        "palette": "blue",
        "menuSounds": "off",
        "splash": False,
        "advancedEmulatorAccess": False,
        "autosaveOnQuit": True,
        "outputDir": str(EXPORT_DIR),
        "target": "macos",
        "runtimeKit": str(menu_shots.KIT),
        "coreCache": str(ROOT / "work/core-cache/macos-arm64"),
    }
    import json

    result = subprocess.run(
        [str(cli()), "export"],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        timeout=180,
        env={**os.environ, "ROMINABOX_GAME_BUNDLE_PREFIX": PREFIX},
    )
    if result.returncode != 0:
        raise SystemExit(f"export failed:\n{result.stdout[-900:]}\n{result.stderr[-400:]}")
    if not APP.is_dir():
        raise SystemExit(f"export wrote no app at {APP}")


def install_disc(app: Path) -> None:
    content = app / "Contents/Resources/content"
    content.mkdir(parents=True, exist_ok=True)
    names = (GDI, *TRACKS)
    for name in names:
        source = DISC / name
        if not source.is_file():
            raise SystemExit(f"disc file missing: {source}")
        destination = content / name
        if destination.exists():
            destination.unlink()
        subprocess.run(["cp", "-c", str(source), str(destination)], check=True, timeout=60)
    for stale in (content / "stub.gdi", content / "stub.bin"):
        if stale.is_file():
            stale.unlink()
    plan_path = app / "Contents/Resources/launch.plan"
    plan = plan_path.read_text()
    old = "content\tcontent/stub.gdi\n"
    new = f"content\tcontent/{GDI}\n"
    if old not in plan:
        raise SystemExit("launch plan does not name the stub sheet this export staged")
    plan_path.write_text(plan.replace(old, new, 1))


def install_player(app: Path, binary: Path, workspace: Path = EXPORT_DIR) -> None:
    entitlements = workspace / "entitlements.plist"
    menu_shots.capture_export_entitlements(app, entitlements)
    retroarch = app / "Contents/MacOS/retroarch"
    retroarch.write_bytes(binary.read_bytes())
    retroarch.chmod(0o755)
    injector = workspace / "inject-dylib"
    subprocess.run(
        ["cc", "-Oz", "-o", str(injector), str(ROOT / "scripts/native_runtime/inject_dylib.c")],
        check=True,
        timeout=30,
    )
    subprocess.run(
        [str(injector), str(retroarch), "@executable_path/librominabox-launch.dylib"],
        check=True,
        timeout=30,
    )
    menu_shots.resign_replaced_player(app, entitlements)


def ensure_flycast_app() -> Path:
    export_stub()
    install_disc(APP)
    use_checkout_player(APP, EXPORT_DIR)
    gdi = APP / "Contents/Resources/content" / GDI
    if not gdi.is_file():
        raise SystemExit(f"exported game has no disc at {gdi}")
    return APP


def bundle_id(app: Path) -> str:
    return subprocess.run(
        ["/usr/bin/plutil", "-extract", "CFBundleIdentifier", "raw", str(app / "Contents/Info.plist")],
        capture_output=True,
        text=True,
        check=True,
        timeout=15,
    ).stdout.strip()


def still_running(app: Path) -> str:
    found = subprocess.run(
        ["pgrep", "-fl", str(app)],
        capture_output=True,
        text=True,
        timeout=15,
    )
    return found.stdout.strip()


def quit_bundle(identifier: str) -> None:
    subprocess.run(
        ["osascript", "-e", f'tell application id "{identifier}" to quit'],
        capture_output=True,
        text=True,
        timeout=20,
    )


def apple_event_quit(app: Path) -> tuple[str, str]:
    """Run under lldb and quit with an Apple Event once the core is loaded.

    We set the frame limit in the player, so a missed quit cannot leave it
    open. Under lldb we stop at the abort and kill the process, so the crash
    dialog never appears.
    """
    identifier = bundle_id(app)
    log = menu_shots.log_of(app)
    if log and log.exists():
        log.unlink()
    player = menu_shots.launcher_of(app)
    bucket: list[str] = []

    def collect(stream) -> None:
        bucket.append(stream.read())

    process = subprocess.Popen(
        [
            "lldb",
            "--batch",
            "-o",
            "process handle SIGBUS SIGSEGV -s false -n false -p true",
            "-o",
            "run",
            "-k",
            "bt",
            "-k",
            "process kill",
            "--",
            str(player),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env={
            **os.environ,
            "ROMINABOX_MAX_FRAMES": "2400",
            "ROMINABOX_VERBOSE": "1",
            "ROMINABOX_MENU_SHOT": str(EXPORT_DIR / "unused-shot.png"),
            "ROMINABOX_GAME_BUNDLE_PREFIX": PREFIX,
            menu_shots.quiet_env(): "1",
        },
    )
    reader = threading.Thread(target=collect, args=(process.stdout,), daemon=True)
    reader.start()
    deadline = time.monotonic() + 100
    sent = False
    core_seen = 0.0
    try:
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
            text = log.read_text(errors="replace") if log and log.exists() else ""
            if not sent and "Loading dynamic libretro core" in text:
                if core_seen == 0.0:
                    core_seen = time.monotonic()
                elif time.monotonic() - core_seen > 8:
                    quit_bundle(identifier)
                    sent = True
            time.sleep(0.4)
        if not sent and process.poll() is None:
            quit_bundle(identifier)
            sent = True
        process.wait(timeout=45)
    finally:
        if process.poll() is None:
            quit_bundle(identifier)
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                pass
        reader.join(timeout=5)
    left = still_running(app)
    if left:
        quit_bundle(identifier)
        time.sleep(1)
        left = still_running(app)
        if left:
            raise SystemExit(f"player still running after quit:\n{left}")
    written = log.read_text(errors="replace") if log and log.exists() else ""
    return "".join(bucket), written


def judge(name: str, debugger: str, log: str, apple_event: bool) -> str | None:
    """Return an empty string when the core was unloaded, or the abort text."""
    tail = debugger[-2000:]
    log_tail = "\n".join(log.splitlines()[-30:])
    if "SIGABRT" in debugger or "abort()" in debugger:
        print(tail)
        print("--- log ---")
        print(log_tail)
        return f"{name}: quit aborted in the loaded core"
    if "exited with status = 0" not in debugger:
        print(tail)
        print("--- log ---")
        print(log_tail)
        return f"{name}: player did not exit 0"
    if "[Core] Unloading core..." not in log:
        print(log_tail)
        return f"{name}: core was not unloaded"
    if apple_event and "[RIB] AppKit quit handed to orderly shutdown." not in log:
        print(log_tail)
        return f"{name}: AppKit quit did not reach the orderly shutdown"
    # In main_exit we ask AppKit to terminate again. A second CMD_EVENT_QUIT
    # at that point would run after the core is gone, and the audio driver
    # would fail to start. Quitting from the pause menu reaches this, because
    # the click has already started the shutdown and this call enters again.
    if "failed_to_start_audio_driver" in log:
        print(log_tail)
        return f"{name}: quit ran command_event again after the core was unloaded"
    print(f"{name}: unloaded the core and exited 0")
    return None


def export_rom(rom: Path, title: str, system: str, workspace: Path) -> Path:
    """Export a single-file game and return the app to launch."""
    require_disk(20.3)
    owned = {
        (ROOT / "work/quit-gba").resolve(): (
            "/Users/mariowilde/src/rominabox-quiet/work/quit-gba",
            "Quit Cartridge.app",
        ),
        (ROOT / "work/quit-ape").resolve(): (
            "/Users/mariowilde/src/rominabox-quiet/work/quit-ape",
            "Quit Ape.app",
        ),
    }
    fresh = owned.get(workspace.resolve())
    if fresh is None:
        raise SystemExit(f"{workspace} is not a directory this scope exports")
    discard_owned(*fresh)
    app = workspace / f"{title}.app"
    workspace.mkdir(parents=True)
    import json
    from built import cli  # noqa: E402

    request = {
        "rom": str(rom),
        "title": title,
        "system": system,
        "showMenu": True,
        "startAtMenu": False,
        "theme": "native",
        "palette": "blue",
        "menuSounds": "off",
        "splash": False,
        "advancedEmulatorAccess": False,
        "autosaveOnQuit": True,
        "outputDir": str(workspace),
        "target": "macos",
        "runtimeKit": str(menu_shots.KIT),
        "coreCache": str(ROOT / "work/core-cache/macos-arm64"),
    }
    result = subprocess.run(
        [str(cli()), "export"],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        timeout=180,
        env={**os.environ, "ROMINABOX_GAME_BUNDLE_PREFIX": PREFIX},
    )
    if result.returncode != 0:
        raise SystemExit(f"export of {title} failed:\n{result.stdout[-900:]}")
    if not app.is_dir():
        raise SystemExit(f"export wrote no app at {app}")
    return app


def scripted_quit(app: Path, script: str) -> tuple[str, str]:
    """Click a menu element under lldb, bounded by the player's frame limit."""
    log = menu_shots.log_of(app)
    if log and log.exists():
        log.unlink()
    player = menu_shots.launcher_of(app)
    bucket: list[str] = []

    def collect(stream) -> None:
        bucket.append(stream.read())

    process = subprocess.Popen(
        [
            "lldb", "--batch",
            "-o", "process handle SIGBUS SIGSEGV -s false -n false -p true",
            "-o", "run",
            "-k", "bt",
            "-k", "process kill",
            "--", str(player),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env={
            **os.environ,
            "ROMINABOX_MAX_FRAMES": "2400",
            "ROMINABOX_VERBOSE": "1",
            "ROMINABOX_MENU_SCRIPT": script,
            "ROMINABOX_MENU_SHOT": str(EXPORT_DIR / "unused-shot.png"),
            "ROMINABOX_GAME_BUNDLE_PREFIX": PREFIX,
            menu_shots.quiet_env(): "1",
        },
    )
    reader = threading.Thread(target=collect, args=(process.stdout,), daemon=True)
    reader.start()
    try:
        process.wait(timeout=90)
    except subprocess.TimeoutExpired:
        quit_bundle(bundle_id(app))
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            pass
    finally:
        reader.join(timeout=5)
    left = still_running(app)
    if left:
        quit_bundle(bundle_id(app))
        time.sleep(1)
        left = still_running(app)
        if left:
            raise SystemExit(f"player still running after quit:\n{left}")
    written = log.read_text(errors="replace") if log and log.exists() else ""
    return "".join(bucket), written


def pause_menu_quit(app: Path) -> tuple[str, str]:
    """Quit from the pause row, with CMD_EVENT_QUIT and not an Apple Event."""
    plan_path = app / "Contents/Resources/launch.plan"
    plan = plan_path.read_text()
    token = "start_at_menu\t0\n"
    if token not in plan:
        raise SystemExit("launch plan is not set to start in the game")
    plan_path.write_text(plan.replace(token, "start_at_menu\t1\n", 1))
    menu_shots.resign_replaced_player(app, EXPORT_DIR / "entitlements.plist")
    try:
        return scripted_quit(app, "wait:20,quit")
    finally:
        plan_path.write_text(plan_path.read_text().replace("start_at_menu\t1\n", token, 1))
        menu_shots.resign_replaced_player(app, EXPORT_DIR / "entitlements.plist")


def main() -> int:
    require_disk(20.2)
    flycast = ensure_flycast_app()
    failed = judge("flycast apple-event", *apple_event_quit(flycast), apple_event=True)
    if failed:
        print(failed)
        return 1
    log = menu_shots.log_of(flycast)
    saved = log.read_text(errors="replace") if log and log.exists() else ""
    if "Auto save state" not in saved or "succeeded" not in saved:
        print("autosave on quit did not succeed")
        return 1
    print("autosave on quit succeeded")

    failed = judge("flycast pause menu", *pause_menu_quit(flycast), apple_event=False)
    if failed:
        print(failed)
        return 1

    advance = Path("/Users/mariowilde/Downloads/roms/Sonic Advance (Europe) (En,Ja,Fr,De,Es).gba")
    gba_dir = ROOT / "work/quit-gba"
    gba = export_rom(advance, "Quit Cartridge", "gba", gba_dir)
    use_checkout_player(gba, gba_dir)
    failed = judge("cartridge apple-event", *apple_event_quit(gba), apple_event=True)
    if failed:
        print(failed)
        return 1

    ape = Path("/Users/mariowilde/Downloads/roms/Ape Escape (Europe).chd")
    ape_dir = ROOT / "work/quit-ape"
    disc = export_rom(ape, "Quit Ape", "ps1", ape_dir)
    use_checkout_player(disc, ape_dir)
    failed = judge("ape escape apple-event", *apple_event_quit(disc), apple_event=True)
    if failed:
        print(failed)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
