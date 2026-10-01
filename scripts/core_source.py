"""Where tests and developer scripts get actual cores, and upstream downloads.

We bundle no cores with the builder, and download each one when an export
requires it. A developer cache in the builder's layout (`cores/`, `licenses/`)
replaces that download cache, so we never reach the network in tests with an
actual core. We fill it with `python3 scripts/prepare_runtime.py --seed-core-cache`,
and in `scripts/test.py` we set ROMINABOX_CORE_SOURCE to it for every scope.
"""

from __future__ import annotations

import os
import platform
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VARIABLE = "ROMINABOX_CORE_SOURCE"
# Archives we download during preparation to read a licence or profiles from.
# We never copy them into the runtime kit.
DOWNLOADS = ROOT / "work/downloads"
# The developer core cache, a folder per target (`seeded_cache`).
CORE_CACHE = ROOT / "work/core-cache"


def host_target() -> str:
    """The default target for builds on this machine."""
    machine = platform.machine()
    system = platform.system()
    architecture = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x86_64", "AMD64": "x86_64"}
    if system == "Darwin":
        return f"macos-{architecture.get(machine, machine)}"
    if system == "Windows":
        return f"windows-{architecture.get(machine, machine)}"
    return f"linux-{architecture.get(machine, machine)}"


def seeded_cache(target: str | None = None) -> Path:
    """The developer cache that we write with `--seed-core-cache` for `target`."""
    return CORE_CACHE / (target or host_target())


def core_source() -> Path:
    """The directory with `cores/` and `licenses/` from which we read cores in tests."""
    named = os.environ.get(VARIABLE)
    return Path(named) if named else seeded_cache()


def core(filename: str) -> Path:
    """One actual core, or an error with the steps to get it."""
    path = core_source() / "cores" / filename
    if not path.is_file():
        raise SystemExit(
            f"{filename} is not in the local core source {core_source()}. "
            "Seed it with: python3 scripts/prepare_runtime.py --seed-core-cache"
        )
    return path
