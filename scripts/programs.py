"""The programs our scripts start, with the same names on every platform.

We run Python children with the interpreter of the current run
(`sys.executable`), so one run never uses two Pythons, and we pass that
interpreter to the callers that are not Python (Rust tests, Node scripts)
in `ROMINABOX_PYTHON`. We look up every other program with `shutil.which`,
because in `subprocess` a bare name does not work for Windows' `npm.cmd`.
"""

from __future__ import annotations

import shutil
import subprocess
import sys

PYTHON = sys.executable

# How we start the programs of a test run. On Windows, a console program
# started from a process without a console gets a new, visible terminal
# window, so each program of a scope would have its own window. When we start
# a scope without a window, the scope and every program under it share one
# hidden console. On macOS and Linux no window opens.
_WINDOWLESS = {
    # The constant exists only in Windows' Python.
    "win32": {"creationflags": getattr(subprocess, "CREATE_NO_WINDOW", 0)},
    "darwin": {},
    "linux": {},
}


def windowless() -> dict:
    """Return the `subprocess` arguments to start a program without a window."""
    if sys.platform not in _WINDOWLESS:
        raise SystemExit(f"how to start a program without a window is not declared for {sys.platform}")
    return dict(_WINDOWLESS[sys.platform])


def find(name: str) -> str | None:
    """Return the program on PATH, or None when it is not installed."""
    return shutil.which(name)


def require(name: str) -> str:
    """Return the program on PATH, or stop the script when it is not installed."""
    found = find(name)
    if found is None:
        raise SystemExit(f"{name} is not on PATH")
    return found
