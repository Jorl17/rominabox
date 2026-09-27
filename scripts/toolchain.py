"""Where the native C and C++ toolchain comes from, for scripts and tests.

In the scripts and the Rust tests we use the POSIX names of the tools: cc,
c++, ar, pkg-config. On macOS and Linux those are already on PATH. On Windows
they come from the MSYS2 UCRT64 environment, in which we build the player.
Its bin directory also contains the runtime libraries of a compiled test
program, so we put it first on PATH. ROMINABOX_MSYS2 is the MSYS2
installation, and when it is unset we use the MSYS2 installer's default.
"""

from __future__ import annotations

import os
from pathlib import Path


def msys2_root() -> Path:
    """Return the MSYS2 installation on Windows."""
    named = os.environ.get("ROMINABOX_MSYS2")
    if named:
        return Path(named)
    return Path(os.environ.get("SystemDrive", "C:") + "\\") / "msys64"


def bin_directories() -> list[Path]:
    """Return the directories to put first on PATH for the toolchain's names."""
    if os.name == "nt":
        return [msys2_root() / "ucrt64" / "bin"]
    if os.name == "posix":
        return []
    raise NotImplementedError(f"no native toolchain for os.name {os.name!r}")


def missing() -> list[Path]:
    """Return the toolchain directories that do not exist, for a clear early message."""
    return [directory for directory in bin_directories() if not directory.is_dir()]


def activate() -> None:
    """Put the toolchain first on this process's PATH, which children inherit."""
    directories = [str(directory) for directory in bin_directories()]
    if directories:
        os.environ["PATH"] = os.pathsep.join([*directories, os.environ.get("PATH", "")])
