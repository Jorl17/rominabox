"""Paths that a run added or rewrote in the account's ROM-in-a-Box folders:
the data of the games and the accounts that QUICK SIGN IN shares.

A shot run without the sandbox writes a game folder here, next to the
player's folders. We compare this tree before and after the tests. On a
second run the folder already exists, so a new path is not enough, and a
file with a changed size or mtime counts as the same leak.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

# The per-user application data folder of each platform, where a game outside
# a sandbox keeps its data and QUICK SIGN IN keeps its accounts, resolved as
# in the launcher (desktop/src-tauri/launcher/accounts_folder.c).
USER_DATA = {
    "darwin": lambda: Path.home() / "Library/Application Support",
    "win32": lambda: Path(os.environ["LOCALAPPDATA"]),
}


def user_data() -> Path:
    if sys.platform not in USER_DATA:
        raise SystemExit(f"no per-user application data folder is declared for {sys.platform}")
    return USER_DATA[sys.platform]()


def watched() -> tuple[Path, Path]:
    """The player's ROM-in-a-Box folder, and the folder where installed
    exports keep the QUICK SIGN IN accounts. A test must never reach either,
    and the exports of a worktree use a separate namespaced accounts folder."""
    return user_data() / "ROM-in-a-Box", user_data() / "ROM-in-a-Box Accounts"

# A file is (size, mtime_ns). A directory is None. Only a file can be rewritten.
Stamp = tuple[int, int] | None


def snapshot() -> dict[str, Stamp]:
    """Every path in the watched folders, relative to the per-user data folder."""
    found: dict[str, Stamp] = {}
    for root in watched():
        if not root.is_dir():
            continue
        found[root.name] = None
        for path in root.rglob("*"):
            relative = str(path.relative_to(user_data()))
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
