"""Check that no file in this repository is a symbolic link.

In a worktree we link to the canonical checkout for the things too big to copy:
node_modules, the preview helper, the built command line. Those links work only
on one machine and must never enter git. A committed link is hard to notice. A
link recorded as pointing at `<canonical
checkout>/desktop/src-tauri/resources/preview` is correct inside a worktree and
points at itself in the canonical checkout.

After a merge into the canonical checkout, links to nothing then replace the
directories, staging stops with "Prepare the offscreen preview helper first",
and unrelated tests fail.

.gitignore does not help once a path is tracked, so the three paths were
committed even though .gitignore listed them.

    python3 scripts/test_no_symlinks.py

This does not prove that the ignore rules are right, or that a worktree has the
right links. It only proves that git contains no links.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def tracked_symlinks() -> list[str]:
    listed = subprocess.run(
        ["git", "ls-files", "-s"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout
    return [
        line.split("\t", 1)[1]
        for line in listed.splitlines()
        if line.startswith("120000 ")
    ]


def main() -> int:
    found = tracked_symlinks()
    if not found:
        print("  ok   git carries no symbolic links")
        return 0
    for path in found:
        target = subprocess.run(
            ["git", "cat-file", "-p", f"HEAD:{path}"],
            cwd=ROOT, capture_output=True, text=True,
        ).stdout.strip()
        print(f"  FAIL {path} is a symbolic link to {target}", file=sys.stderr)
    print(
        f"\n{len(found)} symbolic link(s) are tracked. A link is machine-local: "
        f"correct in the checkout that made it and wrong, or self-referential, "
        f"everywhere else.\n"
        f"  git rm --cached <path>   removes it from git and leaves the file",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
