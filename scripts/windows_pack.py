"""A Windows game made into one program, as we use it in a harness.

A Windows export is one `.exe`, the launcher with the rest of the game packed
after it, in the layout from desktop/crates/rominabox-engine/src/packaging/windows_pack.rs.
On its first launch the game unpacks into its folder under the per-user
application data and runs from there. In a harness we read and write the
game as that folder. With `unpacked` we let the game unpack itself, with a
plan-only launch that starts no player, and return the folder. With
`packed` we list the program's contents without unpacking it.
"""

from __future__ import annotations

import os
import struct
import subprocess
from dataclasses import dataclass
from pathlib import Path

from programs import windowless

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


def runtime_folder(program: Path) -> Path | None:
    """Return the folder that `program` unpacks into, or None if it has no game."""
    pack = packed(program)
    if pack is None:
        return None
    return Path(os.environ["LOCALAPPDATA"]).joinpath(*pack.runtime.split("/"))


def unpacked(program: Path, environment: dict[str, str]) -> Path:
    """Return the folder of the game `program` after it has unpacked itself
    there. `environment` is the quiet harness environment, so no dialog appears."""
    folder = runtime_folder(program)
    if folder is None:
        return program.parent
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
