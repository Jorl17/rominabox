"""A macOS game, as we use it in a harness: an app bundle whose main executable
is the sandboxed player, with its own files in Contents/Resources and its
storage in its HOME inside the sandbox."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path


def _bundle_value(app: Path, key: str) -> str:
    return subprocess.run(
        ["/usr/bin/plutil", "-extract", key, "raw", str(app / "Contents/Info.plist")],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


def launcher(app: Path) -> Path:
    """The bundle's main executable, which is the sandboxed player."""
    executable = app / "Contents/MacOS" / _bundle_value(app, "CFBundleExecutable")
    if executable.is_file() and os.access(executable, os.X_OK):
        return executable
    raise SystemExit(f"no launcher inside {app}")


def resources(app: Path) -> Path:
    return app / "Contents/Resources"


def sandboxed(app: Path) -> bool:
    signed = subprocess.run(
        ["/usr/bin/codesign", "-d", "--entitlements", "-", str(app)],
        capture_output=True,
        text=True,
    )
    return "com.apple.security.app-sandbox" in signed.stdout + signed.stderr


def home(app: Path) -> Path:
    """The HOME of the exported game.

    In App Sandbox, HOME is inside the container. The plan's $user_data is
    Application Support in that HOME, as we resolve it in the launcher.
    """
    if not sandboxed(app):
        return Path.home()
    return Path.home() / "Library/Containers" / _bundle_value(app, "CFBundleIdentifier") / "Data"


def user_data(app: Path) -> Path:
    """The per-user folder the plan's $user_data stands for."""
    return home(app) / "Library/Application Support"


def storage_home(app: Path) -> Path | None:
    """The folder the game's storage lies inside: its container when it is
    sandboxed, and nothing otherwise."""
    return home(app) if sandboxed(app) else None


def prepare_storage(app: Path) -> None:
    """Nothing. In the macOS tests we photograph a game's first launch with its
    shot folder made beforehand, and the picture arrives."""


def running(app: Path) -> str:
    """Processes whose command line contains the app, one "pid command" per line."""
    found = subprocess.run(["pgrep", "-fl", str(app)], capture_output=True, text=True, timeout=15)
    return "\n".join(line for line in found.stdout.splitlines() if "pgrep" not in line)


def unpacked(exported: Path) -> tuple[Path, Path | None]:
    """A macOS game is its bundle: the folder we use in a harness, with no
    separate program."""
    return exported, None


def forget(program: Path, namespace: str) -> None:
    """Refuse, because we never export a macOS game as a separate program, so
    `unpacked` has none to forget. Its bundle goes with its folder."""
    raise SystemExit(f"a macOS game has no program to forget: {program} ({namespace})")


def capture_export_entitlements(app: Path, destination: Path) -> None:
    """Save the sandbox we signed on export before replacing the player binary.

    Replacing Contents/MacOS/retroarch discards that signature, and after
    that there is nothing left to read and put back.
    """
    dumped = subprocess.run(
        ["/usr/bin/codesign", "-d", "--entitlements", str(destination), "--xml", str(app)],
        capture_output=True,
    )
    text = destination.read_text(encoding="utf-8") if destination.is_file() else ""
    if dumped.returncode != 0 or "com.apple.security.app-sandbox" not in text:
        detail = dumped.stderr.decode(errors="replace")[-400:]
        raise SystemExit(
            "the export is not sandboxed, so a shot would not be either\n" + detail
        )


def resign_replaced_player(app: Path, entitlements: Path) -> None:
    """Sign the replacement with the entitlements we wrote on export.

    Signing the new binary with a bare `codesign --sign -` drops the sandbox,
    and `--preserve-metadata=entitlements` on the bundle does not restore it.
    The game's storage would then be under
    ~/Library/Application Support/ROM-in-a-Box instead of in a container.
    """
    if not entitlements.is_file():
        raise SystemExit(f"no entitlements to re-sign with at {entitlements}")
    library = app / "Contents/MacOS/librominabox-launch.dylib"
    if library.is_file():
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(library)], check=True)
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(launcher(app))], check=True)
    subprocess.run(
        [
            "/usr/bin/codesign", "--force", "--sign", "-",
            "--entitlements", str(entitlements),
            str(app),
        ],
        check=True,
    )
