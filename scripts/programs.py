"""The programs our scripts start, with the same names on every platform.

We run Python children with the interpreter of the current run
(`sys.executable`), so one run never uses two Pythons, and we pass that
interpreter to the callers that are not Python (Rust tests, Node scripts)
in `ROMINABOX_PYTHON`. We look up every other program with `shutil.which`,
because in `subprocess` a bare name does not work for Windows' `npm.cmd`.
"""

from __future__ import annotations

import shutil
import sys

PYTHON = sys.executable


def find(name: str) -> str | None:
    """Return the program on PATH, or None when it is not installed."""
    return shutil.which(name)


def require(name: str) -> str:
    """Return the program on PATH, or stop the script when it is not installed."""
    found = find(name)
    if found is None:
        raise SystemExit(f"{name} is not on PATH")
    return found
