"""The path where cargo put a binary we built in this repository.

Parallel checkouts share one cargo target directory, which we export as
`CARGO_TARGET_DIR` in `scripts/worktree.py env`. The build output goes to
`desktop/src-tauri/target` only when that variable is unset. Looking only
there would fail in every worktree, and in the overlay, states and placement
tests we would report the binary as "not built" however recently we built it.

In every script we get the path from here instead of writing it out.
"""

from __future__ import annotations

import os
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_TARGET = ROOT / "desktop/src-tauri/target"


def target_dir() -> Path:
    declared = os.environ.get("CARGO_TARGET_DIR")
    return Path(declared) if declared else DEFAULT_TARGET


def binary(name: str, profile: str = "release") -> Path:
    return target_dir() / profile / name
