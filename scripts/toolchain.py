"""Where the native C and C++ toolchain comes from, for scripts and tests.

In the scripts and the Rust tests we use the POSIX names of the tools: cc,
c++, ar, pkg-config. On macOS and Linux those are already on PATH. On Windows
they come from the MSYS2 UCRT64 environment, in which we build the player.
Its bin directory also contains the runtime libraries of a compiled test
program, so we put it first on PATH. ROMINABOX_MSYS2 is the MSYS2
installation, and when it is unset we use the MSYS2 installer's default.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import core_source  # noqa: E402

# What we compile native test programs with on each platform. In scripts in
# any language we read it here (`python3 scripts/toolchain.py describe`)
# instead of naming a compiler or a flag. With `memoryChecks`, a test program
# stops at its first bad memory access. `supportLibraries` are the libraries
# required at link time by the shared test headers in scripts/native_runtime.
DECLARED = {
    "macos": {"cc": "cc", "cxx": "c++", "memoryChecks": ["-fsanitize=address"], "executableSuffix": "",
              "supportLibraries": []},
    # GCC from MinGW-w64 has no AddressSanitizer runtime, so we run the same
    # tests with the checks on macOS. In test_arguments.h we read the wide
    # command line through shell32.
    "windows": {"cc": "cc", "cxx": "c++", "memoryChecks": [], "executableSuffix": ".exe",
                "supportLibraries": ["-lshell32"]},
}


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


def describe() -> dict:
    """Return the declared toolchain for this machine."""
    platform = core_source.host_target().split("-", 1)[0]
    if platform not in DECLARED:
        raise NotImplementedError(f"no native test toolchain declared for {platform}")
    return DECLARED[platform]


def executable(path: Path) -> Path:
    """Return the name of the file a compiler writes for `path`."""
    return path.with_name(path.name + describe()["executableSuffix"])


if __name__ == "__main__":
    if sys.argv[1:] != ["describe"]:
        raise SystemExit("usage: toolchain.py describe")
    print(json.dumps(describe()))
