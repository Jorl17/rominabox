"""A Windows game made into one program, as we use it in a harness.

A Windows export is one `.exe`, the launcher with the rest of the game packed
after it, in the layout from desktop/src-tauri/src/packaging/windows_pack.rs.
On its first launch the game unpacks into its folder under the per-user
application data and runs from there. In a harness we read and write the
game as that folder. With `unpacked` we let the game unpack itself, with a
plan-only launch that starts no player, and return the folder.
"""

from __future__ import annotations

import os
import struct
import subprocess
from pathlib import Path

from programs import windowless

TRAILER = struct.Struct("<QQ8s")


def runtime_folder(program: Path) -> Path | None:
    """Return the folder that `program` unpacks into, or None if it has no game."""
    with program.open("rb") as file:
        file.seek(0, os.SEEK_END)
        size = file.tell()
        if size < TRAILER.size:
            return None
        file.seek(size - TRAILER.size)
        index_offset, index_size, magic = TRAILER.unpack(file.read(TRAILER.size))
        if magic != b"RIBTAIL1":
            return None
        file.seek(index_offset)
        index = file.read(min(index_size, 4096))
    if index[:8] != b"RIBPACK1":
        raise SystemExit(f"{program} ends with a pack index that is not one")
    (length,) = struct.unpack_from("<H", index, 8)
    runtime = index[10:10 + length].decode("utf-8")
    return Path(os.environ["LOCALAPPDATA"]).joinpath(*runtime.split("/"))


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
