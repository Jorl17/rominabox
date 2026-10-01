"""A Windows game made into one program, as we use it in a harness.

A Windows export is one `.exe`, the launcher with the rest of the game packed
after it, in the layout from desktop/src-tauri/src/packaging/windows_pack.rs.
On its first launch the game unpacks into its folder under the per-user
application data and runs from there. In a harness we read and write the
game as that folder. With `unpacked` we let the game unpack itself, with a
plan-only launch that starts no player, and return the folder. With
`packed` we list the program's contents without unpacking it.

In a harness we never unpack a game into the person's per-user folder.
While an `own_user_data` block runs, every game started in the process uses
a separate folder of the process in its place, for everything outside its
sandbox. Windows keeps the sandbox in the person's Packages folder, and we
remove it with `forget_sandbox`.
"""

from __future__ import annotations

import ctypes
import os
import struct
import subprocess
import tempfile
import threading
from collections.abc import Callable, Iterator, Mapping
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path

from launch_header import TEST_USER_DATA_ENV, launch_declaration
from programs import windowless
from scratch import remove_made

TRAILER = struct.Struct("<QQ8s")


@dataclass
class Pack:
    """Return the contents of a game's program from its index: the folder under
    the per-user application data for unpacking, the launcher's size, and
    each packed file's path in that folder and size once unpacked."""

    runtime: str
    launcher: str
    launcher_bytes: int
    files: list[tuple[str, int]]


def packed(program: Path) -> Pack | None:
    """Return the pack in `program`, or None when it contains no game."""
    with program.open("rb") as file:
        file.seek(0, os.SEEK_END)
        if file.tell() < TRAILER.size:
            return None
        file.seek(-TRAILER.size, os.SEEK_END)
        index_offset, index_size, magic = TRAILER.unpack(file.read(TRAILER.size))
        if magic != b"RIBTAIL1":
            return None
        file.seek(index_offset)
        index = memoryview(file.read(index_size))
    if bytes(index[:8]) != b"RIBPACK1":
        raise SystemExit(f"{program} ends with a pack index that is not one")
    at = 8

    def take(format: str) -> tuple:
        nonlocal at
        values = struct.unpack_from(format, index, at)
        at += struct.calcsize(format)
        return values

    def text() -> str:
        nonlocal at
        (length,) = take("<H")
        at += length
        return bytes(index[at - length:at]).decode("utf-8")

    runtime, launcher = text(), text()
    at += 32
    for _ in range(2):
        (length,) = take("<I")
        at += length
    (count,) = take("<I")
    files = []
    first = index_offset
    for _ in range(count):
        path = text()
        offset, _packed, size = take("<QQQ")
        at += 32 + 1 + 32
        first = min(first, offset)
        files.append((path, size))
    return Pack(runtime, launcher, first, files)


def per_user_folder(environment: Mapping[str, str]) -> Path:
    """Return the per-user folder, outside its sandbox, for the files of a game
    started with `environment` (local_application_data in launcher/windows/main.c):
    the folder set in a test, or %LOCALAPPDATA%."""
    named = environment.get(TEST_USER_DATA_ENV)
    return Path(named) if named else Path(environment["LOCALAPPDATA"])


def runtime_folder(program: Path, environment: Mapping[str, str]) -> Path | None:
    """Return the folder that `program`, started with `environment`, unpacks
    into, or None when it contains no game."""
    pack = packed(program)
    if pack is None:
        return None
    return per_user_folder(environment).joinpath(*pack.runtime.split("/"))


def unpacked(program: Path, environment: dict[str, str]) -> Path:
    """Return the folder of the game `program` after it has unpacked itself
    there. `environment` is the quiet harness environment, so no dialog
    appears, and contains the harness's per-user folder for the unpacking."""
    folder = runtime_folder(program, environment)
    if folder is None:
        return program.parent
    if not environment.get(TEST_USER_DATA_ENV):
        raise SystemExit(f"{program} would unpack into the person's own folders; "
                         "a harness unpacks a game inside own_user_data()")
    subprocess.run(
        [str(program)],
        env=dict(environment, ROMINABOX_PLAN_ONLY="1"),
        stdin=subprocess.DEVNULL,
        capture_output=True,
        check=True,
        timeout=600,
        **windowless(),
    )
    if not folder.is_dir():
        raise SystemExit(f"{program} did not unpack into {folder}")
    return folder


def sandbox_name(identity: str) -> str:
    """Return the sandbox that we register in the launcher for the game `identity`."""
    return launch_declaration("RIB_GAME_APP_ID_PREFIX") + identity


def sandbox_folder(identity: str) -> Path:
    """Return the folder of the sandbox for the game `identity` in the person's
    Packages folder, whatever per-user folder the game used."""
    return Path(os.environ["LOCALAPPDATA"]) / "Packages" / sandbox_name(identity)


def forget_sandbox(identity: str) -> None:
    """Remove the sandbox of the game `identity` in Windows: its registration
    and its folder, with the game's data, as UNINSTALL does in the game
    (forget_if_asked, launcher/windows/main.c)."""
    userenv = ctypes.WinDLL("userenv")
    userenv.DeleteAppContainerProfile.argtypes = [ctypes.c_wchar_p]
    userenv.DeleteAppContainerProfile.restype = ctypes.c_long
    userenv.DeleteAppContainerProfile(sandbox_name(identity))
    folder = sandbox_folder(identity)
    if folder.exists():
        raise SystemExit(f"Windows kept the sandbox's folder {folder} of the game {identity}")


class _OwnUserData:
    """The separate per-user folder of this process while any own_user_data
    block runs: the folder, its identity when we made it, the number of blocks
    running, the variable's value before the first, and whether we kept the
    folder in a block for a player that may still be running in it."""

    lock = threading.Lock()
    folder: Path | None = None
    made: os.stat_result | None = None
    blocks = 0
    before: str | None = None
    kept = False


@contextmanager
def own_user_data() -> Iterator[Callable[[], None]]:
    """While the block runs, every game that this process starts uses a separate
    per-user folder of the process in place of the person's
    (TEST_USER_DATA_ENV). A Windows game unpacks there, and has its QUICK
    SIGN IN folder and everything else outside its sandbox there. Blocks can
    nest and overlap, also in threads, and share the folder, which we remove
    when the last one ends. Yield `keep`, with which we keep the folder for a
    player that may still be running in it."""
    state = _OwnUserData
    with state.lock:
        if state.blocks == 0:
            # A short name, because the deepest file in a game, a CRT Royale
            # texture, is 197 characters below the per-user folder, and in the
            # player we open no path longer than 260. A stamp would exceed that.
            state.folder = Path(tempfile.mkdtemp(prefix="rominabox-"))
            state.made = state.folder.lstat()
            state.before = os.environ.get(TEST_USER_DATA_ENV)
            state.kept = False
            os.environ[TEST_USER_DATA_ENV] = str(state.folder)
        state.blocks += 1

    def keep() -> None:
        state.kept = True

    try:
        yield keep
    finally:
        with state.lock:
            state.blocks -= 1
            if state.blocks == 0:
                if state.before is None:
                    os.environ.pop(TEST_USER_DATA_ENV, None)
                else:
                    os.environ[TEST_USER_DATA_ENV] = state.before
                if state.kept:
                    print(f"retained the per-user folder a timed-out player may be using: {state.folder}")
                else:
                    remove_made(state.folder, state.made)
