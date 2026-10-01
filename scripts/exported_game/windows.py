"""A Windows game, as we use it in a harness: one self-extracting program,
unpacked into a folder under the per-user folder, with its own files in
Resources and its storage in the folder of its sandbox. In a harness we open
one in a per-user folder of the process (windows_pack.own_user_data)."""

from __future__ import annotations

import os
import subprocess
import sys
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import windows_pack  # noqa: E402

from . import launch  # noqa: E402


def launcher(app: Path) -> Path:
    """The game's own program, the one program at the top of its folder:
    named for the game, in a folder named for the game or, unpacked, for its
    identity."""
    programs = sorted(app.glob("*.exe"))
    if len(programs) == 1:
        return programs[0]
    raise SystemExit(f"no launcher inside {app}")


def resources(app: Path) -> Path:
    return app / "Resources"


def _plan(app: Path) -> str:
    return launch.read_plan(resources(app))


def sandboxed(app: Path) -> bool:
    """In a Windows game's launcher we set up the game's sandbox when the plan
    has that option set, as it has in every export."""
    return bool(launch.SANDBOX.search(_plan(app)))


def sandbox_folder(app: Path) -> Path:
    """The sandbox folder of a Windows game, named as we name the sandbox in
    the launcher (windows_pack.sandbox_folder), for the identity in its plan.
    Inside, the game's per-user folder is its AC folder."""
    identity = launch.IDENTITY.search(_plan(app))
    if not identity:
        raise SystemExit(f"cannot name the sandbox of {app}")
    return windows_pack.sandbox_folder(identity.group(1))


def user_data(app: Path) -> Path:
    """The per-user folder the plan's $user_data stands for: the sandbox's AC
    folder, or the per-user folder for a game without one
    (windows_pack.per_user_folder)."""
    return sandbox_folder(app) / "AC" if sandboxed(app) else windows_pack.per_user_folder(os.environ)


def storage_home(app: Path) -> Path:
    """The folder the game's storage lies inside: its sandbox's folder, or
    the games folder in its per-user folder."""
    if sandboxed(app):
        return sandbox_folder(app)
    return windows_pack.per_user_folder(os.environ) / "ROM-in-a-Box" / "Games"


def prepare_storage(app: Path) -> None:
    """Register the game's sandbox with a plan-only launch before we write
    anything into its storage. Registering a sandbox over an existing folder
    empties that folder, including any shot folder we made there, so no
    picture would arrive from a game's first shot."""
    if not sandboxed(app):
        return
    subprocess.run(
        [str(launcher(app))],
        env=dict(os.environ, ROMINABOX_PLAN_ONLY="1", **{launch.QUIET_ENV: "1"}),
        stdin=subprocess.DEVNULL,
        capture_output=True,
        check=True,
        timeout=60,
    )


def running(app: Path) -> str:
    """Processes whose program lies inside the game's folder, one "pid program"
    per line."""
    import ctypes
    from ctypes import wintypes

    psapi = ctypes.WinDLL("psapi")
    kernel32 = ctypes.WinDLL("kernel32")
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
                                                    ctypes.POINTER(wintypes.DWORD)]
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    ids = (wintypes.DWORD * 4096)()
    written = wintypes.DWORD()
    if not psapi.EnumProcesses(ids, ctypes.sizeof(ids), ctypes.byref(written)):
        raise SystemExit("could not list the running processes")
    folder = app.resolve()
    found = []
    for pid in ids[: written.value // ctypes.sizeof(wintypes.DWORD)]:
        process = kernel32.OpenProcess(0x1000, False, pid)  # QUERY_LIMITED_INFORMATION
        if not process:
            continue
        image = ctypes.create_unicode_buffer(32768)
        size = wintypes.DWORD(len(image))
        if kernel32.QueryFullProcessImageNameW(process, 0, image, ctypes.byref(size)):
            if Path(image.value).resolve().is_relative_to(folder):
                found.append(f"{pid} {image.value}")
        kernel32.CloseHandle(process)
    return "\n".join(found)


@contextmanager
def opened(exported: Path) -> Iterator[tuple[Path, Callable[[], None]]]:
    """A Windows game is one program, unpacked into a per-user folder of the
    process (windows_pack.own_user_data). Return the folder it was unpacked
    into, while the block runs, and `keep`. When the block ends, however it
    ends, we remove the game's sandbox and, with the per-user folder,
    everything else from the game, unless we called `keep` for a player that
    may still be running. An export that is already a folder is that folder
    alone."""
    if exported.is_dir():
        yield exported, lambda: None
        return
    with windows_pack.own_user_data() as keep_folder:
        kept = False

        def keep() -> None:
            nonlocal kept
            kept = True
            keep_folder()

        try:
            yield windows_pack.unpacked(exported, dict(os.environ, **{launch.QUIET_ENV: "1"})), keep
        finally:
            if not kept:
                _forget(exported)


def _forget(program: Path) -> None:
    """Remove the sandbox of the Windows game `program`, which is in the
    person's Packages folder with the game's data. We register it in the
    launcher once the game's copy is complete, with the identity from its
    plan. Everything else from the game is in the per-user folder of the
    process, which we remove with it."""
    folder = windows_pack.runtime_folder(program, os.environ)
    identity = launch.IDENTITY.search(_plan(folder)) if folder is not None else None
    if identity:
        windows_pack.forget_sandbox(identity.group(1))
