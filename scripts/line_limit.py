"""Stop a commit that makes one of our own source files too long for a
reader.

    python3 scripts/line_limit.py

We run it in .githooks/pre-commit over what is staged in the repository where
it runs. A hand-written source file may be at most LIMIT lines. LIST contains
the files already longer, once each, with their line counts, and a file that
comes within the limit leaves the list. We stop the commit when a file is
over the limit, new or longer than in LIST, until someone splits it or accepts
it by recording its count in LIST in the same commit, where a reviewer sees
it. When LIST is staged, every entry must contain its file's staged line count.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import PurePosixPath

LIMIT = 1000
LIST = "scripts/fixtures/line-limit.json"
# Our own hand-written source.
EXTENSIONS = {".rs", ".c", ".h", ".cpp", ".hpp", ".m", ".mm", ".ts", ".tsx", ".py", ".mjs", ".js"}
# Not ours or not written by hand: upstream sources and scratch output. We
# also leave out test data under a `fixtures` folder and whatever is marked
# linguist-generated in .gitattributes. Lockfiles have none of EXTENSIONS.
EXCLUDED = ("vendor/", "work/")


def git(*arguments: str, stdin: bytes | None = None) -> bytes:
    return subprocess.run(["git", *arguments], input=stdin, capture_output=True, check=True).stdout


def staged() -> dict[str, str]:
    """Each staged path and its change: A, M, D or T. A rename is a deletion
    and an addition."""
    fields = git("diff", "--cached", "--name-status", "--no-renames", "-z").split(b"\0")[:-1]
    return {path.decode(): status.decode() for status, path in zip(fields[::2], fields[1::2], strict=True)}


def generated(paths: list[str]) -> set[str]:
    """The paths marked linguist-generated in the staged .gitattributes."""
    if not paths:
        return set()
    fields = git("check-attr", "--cached", "-z", "--stdin", "linguist-generated",
                 stdin=b"\0".join(path.encode() for path in paths) + b"\0").split(b"\0")[:-1]
    return {fields[index].decode() for index in range(0, len(fields), 3)
            if fields[index + 2] in (b"set", b"true")}


def limited(paths: list[str]) -> list[str]:
    """Those of `paths` the limit applies to."""
    candidates = [path for path in paths
                  if PurePosixPath(path).suffix in EXTENSIONS
                  and not path.startswith(EXCLUDED)
                  and "fixtures" not in PurePosixPath(path).parts[:-1]]
    excluded = generated(candidates)
    return [path for path in candidates if path not in excluded]


def staged_lines(path: str) -> int | None:
    """The number of lines in the staged `path`, or None when it is not in
    the index."""
    found = subprocess.run(["git", "cat-file", "blob", f":{path}"], capture_output=True)
    if found.returncode != 0:
        return None
    text = found.stdout
    return text.count(b"\n") + (1 if text and not text.endswith(b"\n") else 0)


def listed() -> dict[str, int]:
    """The accepted files and their line counts, as LIST is staged."""
    found = subprocess.run(["git", "cat-file", "blob", f":{LIST}"], capture_output=True)
    if found.returncode != 0:
        return {}
    return json.loads(found.stdout)["files"]


def check() -> tuple[list[str], list[str]]:
    """The problems that stop the commit, and those we only print."""
    changes = staged()
    accepted = listed()
    editing = LIST in changes
    failures: list[str] = []
    notes: list[str] = []
    judged = limited([path for path, change in changes.items() if change != "D"])
    for path in judged:
        lines = staged_lines(path)
        if lines is None:
            raise SystemExit(f"line limit: {path} is staged but not in the index")
        recorded = accepted.get(path)
        if recorded is None:
            if lines > LIMIT:
                failures.append(f"{path} is {lines} lines, over the {LIMIT}-line limit. Split it, or accept it "
                                f"by adding it to {LIST} with {lines}.")
        elif lines > recorded:
            failures.append(f"{path} grew to {lines} lines; {LIST} accepts {recorded}. Split it, or accept "
                            f"the growth by recording {lines} in {LIST}.")
        elif lines <= LIMIT:
            failures.append(f"{path} is {lines} lines, within the {LIMIT}-line limit: take it off {LIST}.")
        elif lines < recorded and editing:
            failures.append(f"{LIST} accepts {recorded} lines for {path}, which is {lines}: record {lines}.")
        elif lines < recorded:
            notes.append(f"{path} is down to {lines} lines; record {lines} in {LIST} to keep it there.")
    for path in accepted:
        if changes.get(path) == "D":
            failures.append(f"{path} is gone: take it off {LIST}.")
    if editing:
        # The entries whose files this commit does not change.
        others = [path for path in accepted if path not in judged and changes.get(path) != "D"]
        within = set(limited(others))
        for path in others:
            lines = staged_lines(path)
            if lines is None:
                failures.append(f"{LIST} lists {path}, which is not in the repository.")
            elif path not in within:
                failures.append(f"{LIST} lists {path}, which the limit does not apply to.")
            elif lines <= LIMIT:
                failures.append(f"{path} is {lines} lines, within the {LIMIT}-line limit: take it off {LIST}.")
            elif lines != accepted[path]:
                failures.append(f"{LIST} accepts {accepted[path]} lines for {path}, which is {lines}: "
                                f"record {lines}.")
    return failures, notes


def main() -> int:
    failures, notes = check()
    for note in notes:
        print(f"line limit: {note}")
    for failure in failures:
        print(f"line limit: {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
