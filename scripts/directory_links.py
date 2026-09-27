"""A folder that leads to another folder: the one kind that an ordinary user
can make on each platform, and how to tell one from a plain folder.

On macOS and Linux we make a symbolic link. On Windows we make a directory
junction, because a symbolic link there requires a privilege most accounts
do not have (WinError 1314). `Path.is_symlink` is false for a junction, so
a check for symbolic links alone would follow it.
"""

from __future__ import annotations

import sys
from pathlib import Path


def _symbolic_link(link: Path, target: Path) -> None:
    link.symlink_to(target, target_is_directory=True)


def _junction(link: Path, target: Path) -> None:
    import _winapi

    _winapi.CreateJunction(str(target), str(link))


MAKERS = {"darwin": _symbolic_link, "linux": _symbolic_link, "win32": _junction}


def link_directory(link: Path, target: Path) -> None:
    """Make `link` lead to the directory `target`."""
    if sys.platform not in MAKERS:
        raise SystemExit(f"no way to link a folder is declared for {sys.platform}")
    MAKERS[sys.platform](link, target)


def redirected(path: Path) -> bool:
    """Remove a symbolic link or a junction by unlinking it, never by deleting
    what it leads to."""
    return path.is_symlink() or path.is_junction()
