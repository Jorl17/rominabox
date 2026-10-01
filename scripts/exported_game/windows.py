"""A Windows game, as we use it in a harness: one self-extracting program,
unpacked into a folder under the per-user application data, with its own
files in Resources and its storage in the folder of its sandbox."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import windows_pack  # noqa: E402
from programs import windowless  # noqa: E402

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
    the launcher: the application id prefix declared for the launcher and
    the player in vendor/retroarch/rominabox_launch.h, and the game's identity.
    Inside, the game's per-user folder is its AC folder."""
    identity = launch.IDENTITY.search(_plan(app))
    if not identity:
        raise SystemExit(f"cannot name the sandbox of {app}")
    prefix = launch.launch_declaration("RIB_GAME_APP_ID_PREFIX")
    return Path(os.environ["LOCALAPPDATA"]) / "Packages" / f"{prefix}{identity.group(1)}"


def user_data(app: Path) -> Path:
    """The per-user folder the plan's $user_data stands for: the sandbox's AC
    folder, or the per-user application data of a game without one."""
    return sandbox_folder(app) / "AC" if sandboxed(app) else Path(os.environ["LOCALAPPDATA"])


def storage_home(app: Path) -> Path:
    """The folder the game's storage lies inside: its sandbox's folder, or
    the games folder of the per-user application data."""
    if sandboxed(app):
        return sandbox_folder(app)
    return Path(os.environ["LOCALAPPDATA"]) / "ROM-in-a-Box" / "Games"


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


def unpacked(exported: Path) -> tuple[Path, Path | None]:
    """A Windows game is one program: the folder it is unpacked into, which we
    use in a harness, and the program a person opens. An export that is
    already a folder is that folder alone."""
    if not exported.is_file():
        return exported, None
    return windows_pack.unpacked(exported, dict(os.environ, **{launch.QUIET_ENV: "1"})), exported


def forget(program: Path, namespace: str) -> None:
    """Remove everything on this computer from the Windows game `program` with
    the game's own UNINSTALL. We write the same marker as its menu into its
    data folder and launch it, and before a core is loaded we remove in the
    launcher the game's sandbox, its data and every unpacked copy
    (forget_if_asked, launcher/windows/main.c). We also remove the
    QUICK SIGN IN folder of a game exported under `namespace`, but keep the
    shared one and the one for a worktree."""
    environment = dict(os.environ, **{launch.QUIET_ENV: "1"})
    folder = windows_pack.unpacked(program, environment)
    data = launch.data_dir(_plan(folder), user_data(folder))
    accounts = launch.ACCOUNTS_DIR.search(_plan(folder))
    if data is not None:
        data.mkdir(parents=True, exist_ok=True)
        (data / launch.launch_declaration("RIB_FORGET_MARKER")).write_bytes(b"")
        subprocess.run(
            [str(program)],
            env=dict(environment, ROMINABOX_PLAN_ONLY="1"),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            check=True,
            timeout=600,
            **windowless(),
        )
    if accounts and namespace and accounts.group(1).endswith(f"-{namespace}") \
            and accounts.group(1) != os.environ.get("ROMINABOX_ACCOUNTS_FOLDER"):
        local = Path(os.environ["LOCALAPPDATA"])
        made = local / accounts.group(1)
        if made.parent == local and made.is_dir() and not made.is_symlink():
            shutil.rmtree(made)
