"""Report what every parallel worktree is doing, and which ones to look at.

We report the state of every worktree, so that someone notices a stopped
one without having to remember to look.

    uv run python scripts/tracks.py            # one line per worktree, problems first
    uv run python scripts/tracks.py --quiet    # only the worktrees that need attention

We only report and never act. Merging, restarting and removing stay manual.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Below this much free disk space, stop and clear something.
DISK_FLOOR_GB = 20
# We report a worktree as stopped after this long with no commit and no
# process running.
QUIET_MINUTES = 25


def run(*args: str, cwd: Path | None = None) -> str:
    done = subprocess.run(args, cwd=cwd or ROOT, capture_output=True, text=True)
    return done.stdout.strip() if done.returncode == 0 else ""


def worktrees() -> list[Path]:
    found = []
    for line in run("git", "worktree", "list", "--porcelain").splitlines():
        if line.startswith("worktree "):
            path = Path(line[len("worktree "):])
            if path != ROOT:
                found.append(path)
    return sorted(found)


def agents_running() -> str:
    """Return the running cursor-agent processes, to show a stopped worktree as stopped.

    We see only cursor-agent processes. A process on another machine does not
    appear, so we show a worktree used that way as idle. Its staged but
    uncommitted files show that it is in use.
    """
    listed = subprocess.run(
        ["pgrep", "-fl", "cursor-agent"], capture_output=True, text=True
    ).stdout
    return listed


def free_gb() -> float:
    usage = shutil.disk_usage("/")
    return usage.free / 1_000_000_000


def look(path: Path, merged: set[str], busy: str) -> dict:
    branch = run("git", "rev-parse", "--abbrev-ref", "HEAD", cwd=path)
    when = run("git", "log", "-1", "--format=%ct", cwd=path)
    quiet = (time.time() - int(when)) / 60 if when.isdigit() else None
    dirty = bool(run("git", "status", "--porcelain", "--untracked-files=no", cwd=path))
    # The command line of a worktree's process contains its directory.
    working = path.name in busy
    reports = sorted((path / "private/docs/reports").glob("*.md")) if (path / "private/docs/reports").is_dir() else []
    # A report in work/ is lost when we remove the checkout, because git
    # ignores that directory. We count only the reports of this branch. Every
    # worktree has leftovers from branches merged earlier, and if we flagged
    # those, they would hide the important ones.
    already = {q.stem.lower() for q in (ROOT / "private/docs/reports").glob("*.md")}
    stranded = [
        q
        for q in sorted((path / "work").glob("*.md"))
        if (path / "work").is_dir() and q.stem.lower().replace("-", "") not in
        {name.replace("-", "") for name in already}
    ]

    trouble = []
    # A branch with no commits beyond its base is "merged" by definition, as
    # in every freshly created worktree. We do not report such a checkout as
    # removable, because work may be in progress in it.
    if branch in merged and not working:
        trouble.append("merged into main — its checkout can go")
    elif not working and quiet is not None and quiet > QUIET_MINUTES:
        trouble.append(f"nothing running and no commit for {quiet:.0f} minutes")
    if dirty:
        trouble.append("uncommitted work")
    if stranded:
        trouble.append(
            f"report still in work/, which git ignores: {', '.join(p.name for p in stranded)}"
        )
    return {
        "branch": path.name.replace("rominabox-", ""),
        "branch": branch,
        "quiet_minutes": quiet,
        "working": working,
        "reports": len(reports),
        "trouble": trouble,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quiet", action="store_true", help="only what needs attention")
    parser.add_argument("--json", action="store_true", help="for something else to read")
    arguments = parser.parse_args()

    merged = {
        line.strip().lstrip("+* ").strip()
        for line in run("git", "branch", "--merged", "main").splitlines()
    }
    busy = agents_running()
    tracks = [look(path, merged, busy) for path in worktrees()]
    disk = free_gb()

    if arguments.json:
        print(json.dumps({"disk_free_gb": round(disk, 1), "tracks": tracks}, indent=2))
        return 0

    if disk < DISK_FLOOR_GB:
        print(f"DISK  {disk:.0f} GB free, below the {DISK_FLOOR_GB} GB line. "
              f"Clear a merged worktree before anything else.", file=sys.stderr)
    else:
        print(f"disk  {disk:.0f} GB free")

    needing = [t for t in tracks if t["trouble"]]
    for track in tracks:
        if arguments.quiet and not track["trouble"]:
            continue
        state = "working" if track["working"] else "idle"
        quiet = f"{track['quiet_minutes']:.0f}m" if track["quiet_minutes"] is not None else "?"
        print(f"\n  {track['track']:<16}{state:<9}last commit {quiet} ago")
        for line in track["trouble"]:
            print(f"      - {line}")

    if needing:
        print(f"\n{len(needing)} worktree(s) need attention.")
    elif not arguments.quiet:
        print("\nevery worktree is either active or merged")
    return 0


if __name__ == "__main__":
    sys.exit(main())
