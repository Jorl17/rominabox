"""Free space on the volume a script is about to write to, found at run time.

In scripts that export games or build RmlUi we stop before the disk is full.
The volume to check is the one with the directory we write into, and we
find it from that directory at run time.
"""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path


def free_gigabytes(directory: Path) -> float:
    """Free space, in GB, on the volume with `directory`.

    We will create a directory that does not exist yet on the volume of its
    nearest existing parent, so we check that volume.
    """
    probe = Path(directory).absolute()
    while not probe.exists() and probe != probe.parent:
        probe = probe.parent
    return shutil.disk_usage(probe).free / 1024**3


def require(floor: float, *directories: Path) -> None:
    """Stop unless every volume we write to has `floor` GB free.

    With no directory given, we check the temporary directory, where exports
    and scratch builds go.
    """
    for directory in directories or (Path(tempfile.gettempdir()),):
        free = free_gigabytes(directory)
        if free < floor:
            raise SystemExit(
                f"{free:.1f} GB free on the volume holding {directory}, below {floor:g}; stopping"
            )
