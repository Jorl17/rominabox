"""New names under $TMPDIR, starting with rominabox, from one run.

We compare only what appeared during this run, so we do not count a
directory that another process already had open.
"""

from __future__ import annotations

import os
import tempfile
from pathlib import Path


def directory() -> Path:
    return Path(tempfile.gettempdir())


def snapshot() -> frozenset[str]:
    root = directory()
    if not root.is_dir():
        return frozenset()
    names: set[str] = set()
    with os.scandir(root) as entries:
        for entry in entries:
            if entry.name.startswith("rominabox"):
                names.add(entry.name)
    return frozenset(names)


def additions(before: frozenset[str], after: frozenset[str]) -> list[str]:
    return sorted(after - before)
