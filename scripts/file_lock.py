"""One lock across processes, on a file, on Windows and on POSIX systems.

We take it in scripts that build shared output in parallel (the RmlUi recipe,
the menu harness objects), so we wait in the second script and then use the
work already done.
"""

from __future__ import annotations

import os
from typing import IO


def hold_exclusively(handle: IO) -> None:
    """Block until this process has the lock on `handle`. Closing it releases the lock."""
    if os.name == "nt":
        import msvcrt

        # With LK_LOCK, waiting ends after about ten seconds. A build can take longer.
        while True:
            try:
                msvcrt.locking(handle.fileno(), msvcrt.LK_LOCK, 1)
                return
            except OSError:
                continue
    elif os.name == "posix":
        import fcntl

        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
    else:
        raise NotImplementedError(f"no file lock for os.name {os.name!r}")
