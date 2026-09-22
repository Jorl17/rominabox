"""Merge a finished branch, or reject it and print the reasons.

We merge a branch when its suite is green and the whole suite is still green
with the branch merged into the target tree. We run both checks here, so we
never merge a branch without running its tests in that tree.

    python3 scripts/merge_track.py options          # check, then merge
    python3 scripts/merge_track.py options --check  # say whether it is ready
    python3 scripts/merge_track.py --all --check    # every track at once

The reasons for rejecting a branch, and why each one is important:

  uncommitted work   A branch with a dirty worktree is not finished, and
                     merging it would merge an incomplete version.
  no tag             We tag every working step, so that someone can review
                     the steps one by one instead of as one lump.
  no report          A report contains what was done and what is still
                     wrong, and someone can read it without the diff.
  its suite red      The suite of the branch must be green in the branch
                     before we test the tree it is merged into.
  the tree goes red  We undo the merge. In this way we catch two branches
                     that are each green alone and red together.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRACKS = [
    "identification", "achievements", "options", "volume", "shaders",
    "bios", "dualsense", "nesblur", "heldkey",
]


def git(where: Path, *arguments: str) -> str:
    done = subprocess.run(
        ["git", "-C", str(where), *arguments], capture_output=True, text=True
    )
    if done.returncode != 0:
        raise SystemExit(f"git {' '.join(arguments)}: {done.stderr.strip()}")
    return done.stdout.strip()


def worktree(track: str) -> Path:
    return ROOT.parent / f"rominabox-{track}"


def readiness(track: str) -> tuple[bool, list[str]]:
    """Whether we may merge a branch, and every reason against it."""
    where = worktree(track)
    if not where.is_dir():
        return False, ["it has no worktree"]

    problems: list[str] = []
    # The symlinked runtime always appears as untracked and is not work on the branch.
    dirty = [
        line
        for line in git(where, "status", "--short").splitlines()
        if line.strip() and "resources/" not in line
    ]
    if dirty:
        problems.append(f"{len(dirty)} file(s) are uncommitted, so it is not finished")
    if not git(where, "tag", "--list", f"{track}/*"):
        problems.append("it tagged no working step")
    reports = list((where / "work").glob("*.md")) if (where / "work").is_dir() else []
    if not reports:
        problems.append("it wrote no report under work/")
    if not git(where, "log", "--oneline", "main..HEAD"):
        problems.append("it committed nothing")
    return not problems, problems


def suite(where: Path) -> bool:
    print(f"  running the suite in {where.name} ...", flush=True)
    done = subprocess.run(
        [sys.executable, "scripts/test.py", "--all"], cwd=where, capture_output=True, text=True
    )
    if done.returncode != 0:
        for line in (done.stdout + done.stderr).splitlines():
            if line.startswith("FAIL"):
                print(f"    {line}")
    return done.returncode == 0


def merge(track: str) -> int:
    ready, problems = readiness(track)
    if not ready:
        for problem in problems:
            print(f"  not ready: {problem}", file=sys.stderr)
        return 1
    where = worktree(track)

    # First merge the target tree into the branch. An older branch does not
    # contain the work merged since, so as it is, its suite could be red
    # because of that work, or green without a test against it. Conflicts
    # then appear here, in the worktree of the branch, and not halfway
    # through a merge on main.
    print(f"  bringing main into {where.name} ...")
    brought = subprocess.run(
        ["git", "-C", str(where), "merge", "--no-edit", "main"],
        capture_output=True, text=True,
    )
    # The fork is a submodule. Merging main moves the pointer, but the checkout
    # of the fork in the worktree stays where it was, and the difference
    # appears as an uncommitted file that looks like unfinished work.
    if brought.returncode == 0:
        subprocess.run(
            ["git", "-C", str(where), "submodule", "update", "--init", "--recursive"],
            capture_output=True, text=True,
        )
    if brought.returncode != 0:
        print(
            f"  {track} conflicts with main:\n{brought.stdout}{brought.stderr}",
            file=sys.stderr,
        )
        subprocess.run(["git", "-C", str(where), "merge", "--abort"], capture_output=True)
        return 1

    # In several test scopes we drive the exporter through its command line, so
    # the binary must exist in the tree under test. Without it we get an empty
    # error that looks like a fault of the branch.
    print(f"  building the exporter in {where.name} ...", flush=True)
    built = subprocess.run(
        ["cargo", "build", "--release", "--quiet",
         "--manifest-path", "desktop/src-tauri/Cargo.toml", "--bin", "rominabox-cli"],
        cwd=where, capture_output=True, text=True,
    )
    if built.returncode != 0:
        print(f"  {track} does not build:\n{built.stderr[-1200:]}", file=sys.stderr)
        return 1

    if not suite(where):
        print(
            f"  {track}'s suite is red with main merged in; it does not merge",
            file=sys.stderr,
        )
        return 1

    before = git(ROOT, "rev-parse", "HEAD")
    branch = git(where, "rev-parse", "--abbrev-ref", "HEAD")
    print(f"  merging {branch} ...")
    done = subprocess.run(
        ["git", "-C", str(ROOT), "merge", "--no-ff", branch, "-m",
         f"Merge the {track} branch\n\nIts own suite was green and the whole suite is green with it in."],
        capture_output=True, text=True,
    )
    if done.returncode != 0:
        print(f"  the merge conflicts:\n{done.stdout}{done.stderr}", file=sys.stderr)
        subprocess.run(["git", "-C", str(ROOT), "merge", "--abort"], capture_output=True)
        return 1
    if not suite(ROOT):
        print(
            f"  {track} passes alone and the tree fails with it in. Undone.",
            file=sys.stderr,
        )
        subprocess.run(["git", "-C", str(ROOT), "reset", "--hard", before], capture_output=True)
        return 1
    print(f"  merged {track}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("track", nargs="?", choices=TRACKS)
    parser.add_argument("--all", action="store_true", help="every branch")
    parser.add_argument("--check", action="store_true", help="say what is ready, merge nothing")
    arguments = parser.parse_args()
    if not arguments.track and not arguments.all:
        parser.error("name a branch, or --all")

    chosen = TRACKS if arguments.all else [arguments.track]
    if arguments.check:
        waiting = 0
        for track in chosen:
            ready, problems = readiness(track)
            print(f"{track:<16}{'ready' if ready else '; '.join(problems)}")
            waiting += 0 if ready else 1
        print(f"\n{len(chosen) - waiting} of {len(chosen)} ready to merge")
        return 0

    failures = 0
    for track in chosen:
        print(f"== {track}")
        failures += merge(track)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
