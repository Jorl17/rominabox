"""Check that quitting unloads the core before the process exits.

Cmd-Q, the menu bar Quit, closing the last window and an Apple Event quit
all lead to applicationShouldTerminate. If we returned NSTerminateNow,
exit() would run on that stack, and the Flycast static destructors would
abort while its threads were still running. So in the draw observer we run
main_exit before exit. We export the generated cartridge, launch it under
lldb and send the Apple Event, so that a regression shows up as the abort
and not as a changed string.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import free_space  # noqa: E402
import menu_shots  # noqa: E402
from core_source import core_source  # noqa: E402

# Every directory that we write in an export of the quit tests, declared once.
OWN_WORKSPACES = (ROOT / "work/quit-gbc",)
PREFIX = "app.rominabox.game.wt-quit"


def require_disk(floor: float) -> None:
    """The places where we require free space: the temporary directory, for
    the exports, and the directories of the quit tests."""
    free_space.require(floor, Path(tempfile.gettempdir()), *OWN_WORKSPACES)


def use_checkout_player(app: Path, workspace: Path) -> None:
    retroarch = app / "Contents/MacOS/retroarch"
    if not retroarch.is_file():
        raise SystemExit(f"exported game has no player at {retroarch}")
    # For the pause row we edit launch.plan, and to sign again we need the
    # entitlements from the export. After a replacement there is nothing to read.
    install_player(app, menu_shots.built_player(), workspace)


def install_player(app: Path, binary: Path, workspace: Path) -> None:
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
    # For a named run we export again, so that the app has the current launcher.
    # We only ever remove the directories of these tests.
    if workspace.resolve() not in {path.resolve() for path in OWN_WORKSPACES}:
        raise SystemExit(f"{workspace} is not a directory this scope exports")
    if workspace.is_symlink() or (workspace.exists() and not workspace.is_dir()):
        raise SystemExit(f"{workspace} is not this scope's directory")
    if workspace.exists():
        shutil.rmtree(workspace)
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
        "coreCache": str(core_source()),
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


def ready(name: str) -> Path | None:
    """Return the fetched or committed file, or None after printing the skip line.

    A missing disc is not a failed quit, but we still report it.
    """
    import fetch_test_content

    path, reason = fetch_test_content.locate(name)
    if path is None:
        print(f"skipped: {name} not available ({reason})", flush=True)
        return None
    return path


def exported(rom: Path, title: str, system: str, workspace: Path) -> Path:
    app = export_rom(rom, title, system, workspace)
    use_checkout_player(app, workspace)
    return app


def main() -> int:
    require_disk(20.2)
    failed = False

    # We use the generated cartridge as "a game that boots", so no test
    # requires a commercial game.
    cartridge = ready("test-game")
    if cartridge is not None:
        app = exported(cartridge, "Quit Cartridge", "gbc", ROOT / "work/quit-gbc")
        problem = judge("cartridge apple-event", *apple_event_quit(app), apple_event=True)
        if problem:
            print(problem)
            failed = True

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
