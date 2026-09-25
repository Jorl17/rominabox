"""Paths that a run added or rewrote in the account's ROM-in-a-Box folders:
the data of the games and the accounts that QUICK SIGN IN shares.

A shot run without the sandbox writes a game folder here, next to the
player's folders. We compare this tree before and after the tests. On a
second run the folder already exists, so a new path is not enough, and a
file with a changed size or mtime counts as the same leak.
"""

from __future__ import annotations

from pathlib import Path

APPLICATION_SUPPORT = Path.home() / "Library/Application Support"
REAL_SUPPORT = APPLICATION_SUPPORT / "ROM-in-a-Box"
# Where installed exports keep the QUICK SIGN IN accounts. A test must never
# reach it, and the exports of a worktree use a separate namespaced folder.
REAL_ACCOUNTS = APPLICATION_SUPPORT / "ROM-in-a-Box Accounts"
WATCHED = (REAL_SUPPORT, REAL_ACCOUNTS)

# A file is (size, mtime_ns). A directory is None. Only a file can be rewritten.
Stamp = tuple[int, int] | None


def snapshot() -> dict[str, Stamp]:
    """Every path in the watched folders, relative to Application Support."""
    found: dict[str, Stamp] = {}
    for root in WATCHED:
        if not root.is_dir():
            continue
        found[root.name] = None
        for path in root.rglob("*"):
            relative = str(path.relative_to(APPLICATION_SUPPORT))
            if path.is_file():
                info = path.stat()
                found[relative] = (info.st_size, info.st_mtime_ns)
            else:
                found[relative] = None
    return found


def _names(tree: dict[str, Stamp] | frozenset[str] | set[str]) -> set[str]:
    if isinstance(tree, dict):
        return set(tree)
    return set(tree)


def additions(
    before: dict[str, Stamp] | frozenset[str] | set[str],
    after: dict[str, Stamp] | frozenset[str] | set[str],
) -> list[str]:
    return sorted(_names(after) - _names(before))


def modifications(before: dict[str, Stamp], after: dict[str, Stamp]) -> list[str]:
    """Files that were already there and whose size or mtime changed."""
    changed = []
    for path, previous in before.items():
        if previous is None:
            continue
        current = after.get(path)
        if isinstance(current, tuple) and current != previous:
            changed.append(path)
    return sorted(changed)
