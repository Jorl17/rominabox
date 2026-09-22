"""Paths that a run added in the account's ROM-in-a-Box support directory.

A shot run without the sandbox writes a game folder here, next to the
player's folders. We compare this tree before and after the tests.
"""

from __future__ import annotations

from pathlib import Path

REAL_SUPPORT = Path.home() / "Library/Application Support/ROM-in-a-Box"


def snapshot() -> frozenset[str]:
    if not REAL_SUPPORT.is_dir():
        return frozenset()
    return frozenset(str(path.relative_to(REAL_SUPPORT)) for path in REAL_SUPPORT.rglob("*"))


def additions(before: frozenset[str], after: frozenset[str]) -> list[str]:
    return sorted(after - before)
