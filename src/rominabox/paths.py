import os
import sys
from pathlib import Path


def data_home() -> Path:
    """Return the platform's persistent application-data directory."""
    if sys.platform == "darwin":
        return Path.home() / "Library/Application Support/ROM-in-a-Box"
    if sys.platform == "win32":
        return Path(os.environ.get("LOCALAPPDATA", str(Path.home()))) / "ROM-in-a-Box"
    return Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "rominabox"


def resources() -> Path:
    """Locate mutable package resources without depending on the current directory."""
    if getattr(sys, "frozen", False):
        exe = Path(sys.executable).resolve()
        if sys.platform == "darwin":
            return exe.parent.parent / "Resources"
        return exe.parent
    return Path(__file__).resolve().parents[2] / "work"


def runtime_kit() -> Path:
    """Find the supplied runtime kit, allowing an explicit developer override."""
    return Path(os.environ.get("ROMINABOX_RUNTIME_KIT", str(resources() / "runtime-kit")))
