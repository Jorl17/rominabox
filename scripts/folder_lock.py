"""A lock on a folder, which we take by creating a directory atomically.

We must not choose the same port offset in two worktree commands, or write
into one build folder from two player builds. `mkdir` is atomic on every
platform, and `flock` is not reliable across platforms.
"""

from __future__ import annotations

import os
import shutil
import time
from pathlib import Path

import processes


class Lock:
    """Held while the directory `path` exists. We take back a lock whose owner
    has ended, and wait up to `wait` seconds for one in use before we stop
    with `busy`."""

    def __init__(self, path: Path, busy: str | None = None, wait: float = 30):
        self.path = path
        self.busy = busy or f"another command has held {path} for {wait:g}s"
        self.wait = wait
        self.held = False

    def __enter__(self) -> "Lock":
        deadline = time.monotonic() + self.wait
        while True:
            try:
                self.path.mkdir(parents=True)
                (self.path / "pid").write_text(str(os.getpid()))
                self.held = True
                return self
            except FileExistsError:
                if self._stale():
                    continue
                if time.monotonic() >= deadline:
                    raise SystemExit(self.busy)
                time.sleep(0.2)

    def _stale(self) -> bool:
        """Reclaim a lock whose owner process has ended, without taking one in use."""
        pid_file = self.path / "pid"
        try:
            holder = int(pid_file.read_text().strip())
        except (OSError, ValueError):
            return False
        if processes.alive(holder):
            return False
        # We move it aside and check again, so we never delete a lock that a
        # new owner has just taken.
        aside = self.path.with_suffix(f".stale.{os.getpid()}")
        try:
            self.path.rename(aside)
        except OSError:
            return False
        try:
            if int((aside / "pid").read_text().strip()) != holder:
                aside.rename(self.path)
                return False
        except (OSError, ValueError):
            pass
        shutil.rmtree(aside, ignore_errors=True)
        return True

    def __exit__(self, *_: object) -> None:
        if self.held:
            shutil.rmtree(self.path, ignore_errors=True)
