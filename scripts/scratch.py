"""A temporary directory that we remove when the block ends, also on failure.

With `mkdtemp`, a `rominabox-*` directory stays in `$TMPDIR` whenever the line
that removes it does not run. We use `TemporaryDirectory` for the removal,
and tests get their directories here instead of from `mkdtemp`.
"""

from __future__ import annotations

import os
import shutil
import stat
import tempfile
from pathlib import Path


def scratch_run() -> str:
    """Return the stamp of this run, which is in the name of every temporary
    folder we make. We report only names that contain it. It is the same
    stamp as on the Rust side."""
    run = os.environ.get("ROMINABOX_SCRATCH_RUN", "direct")
    if not run or "/" in run or "\\" in run or ".." in run:
        raise ValueError(f"scratch run id must be one path component, got {run!r}")
    return run


def scratch(prefix: str = "rominabox-") -> tempfile.TemporaryDirectory:
    if not prefix or "/" in prefix or "\\" in prefix or ".." in prefix:
        raise ValueError(f"scratch prefix must be one path component, got {prefix!r}")
    return tempfile.TemporaryDirectory(prefix=f"{prefix}{scratch_run()}-")


def remove_made(folder: Path, made: os.stat_result) -> None:
    """Remove `folder`, a temporary folder this process made, whose lstat
    was `made` at the time. Refuse when the path now leads to anything else."""
    current = folder.lstat()
    if not stat.S_ISDIR(current.st_mode) or current.st_dev != made.st_dev or current.st_ino != made.st_ino:
        raise RuntimeError(f"temporary folder changed ownership: {folder}")
    shutil.rmtree(folder)
