"""Test the pre-commit line limit on repositories that we make for the test.

    python3 scripts/test_line_limit.py

In each case we make a git repository in a temporary directory, commit a
starting state, stage a change and run scripts/line_limit.py there, as in
.githooks/pre-commit. We do not read or stage the files of this repository.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import line_limit  # noqa: E402
import scratch  # noqa: E402

SCRIPT = Path(__file__).resolve().parent / "line_limit.py"
FAILURES: list[str] = []
# We make these repositories here, so we commit unsigned and under any name.
IDENTITY = ["-c", "user.name=line limit test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false"]


def check(condition: bool, message: str, detail: str = "") -> None:
    if condition:
        print(f"  ok   {message}")
    else:
        print(f"  FAIL {message}\n{detail}")
        FAILURES.append(message)


class Repository:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.git("init", "-q")

    def git(self, *arguments: str) -> None:
        subprocess.run(["git", *IDENTITY, *arguments], cwd=self.root, check=True, capture_output=True)

    def write(self, path: str, lines: int) -> None:
        file = self.root / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text("".join(f"line {number}\n" for number in range(lines)))

    def accept(self, files: dict[str, int]) -> None:
        file = self.root / line_limit.LIST
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(json.dumps({"files": files}))

    def commit(self) -> None:
        """Commit what is there now as the starting state for a change."""
        self.git("add", "-A")
        self.git("commit", "-q", "--no-verify", "--allow-empty", "-m", "start")

    def stage(self) -> subprocess.CompletedProcess[str]:
        self.git("add", "-A")
        return self.run()

    def run(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, str(SCRIPT)], cwd=self.root, capture_output=True, text=True)


def repository(case) -> None:
    with scratch.scratch("rominabox-line-limit-") as made:
        case(Repository(Path(made)))


def stopped(result: subprocess.CompletedProcess[str], *words: str) -> bool:
    return result.returncode == 1 and all(word in result.stderr for word in words)


def passed(result: subprocess.CompletedProcess[str]) -> bool:
    return result.returncode == 0 and not result.stderr


def shown(result: subprocess.CompletedProcess[str]) -> str:
    return f"       exit {result.returncode}\n       {result.stdout}{result.stderr}"


def a_new_file_within_the_limit_passes(repo: Repository) -> None:
    repo.write("src/small.rs", line_limit.LIMIT)
    result = repo.stage()
    check(passed(result), f"a new file of {line_limit.LIMIT} lines passes", shown(result))


def a_new_file_over_the_limit_names_itself_until_it_is_listed(repo: Repository) -> None:
    repo.write("src/big.rs", line_limit.LIMIT + 1)
    result = repo.stage()
    check(stopped(result, "src/big.rs", str(line_limit.LIMIT + 1), line_limit.LIST),
          "a new file over the limit stops the commit with its name and line count", shown(result))
    repo.accept({"src/big.rs": line_limit.LIMIT + 1})
    result = repo.stage()
    check(passed(result), "listing it in the same commit accepts it", shown(result))


def a_listed_file_grows_only_by_recording_it(repo: Repository) -> None:
    repo.write("app/App.tsx", 1500)
    repo.accept({"app/App.tsx": 1500})
    repo.commit()
    repo.write("app/App.tsx", 1501)
    result = repo.stage()
    check(stopped(result, "app/App.tsx", "1501", "1500"), "a listed file that grows stops the commit",
          shown(result))
    repo.accept({"app/App.tsx": 1501})
    result = repo.stage()
    check(passed(result), "recording its new count in the same commit accepts the growth", shown(result))


def a_listed_file_that_shrinks_is_told_to_record_it(repo: Repository) -> None:
    repo.write("app/App.tsx", 1500)
    repo.accept({"app/App.tsx": 1500})
    repo.commit()
    repo.write("app/App.tsx", 1400)
    result = repo.stage()
    check(result.returncode == 0 and "1400" in result.stdout,
          "a listed file that shrinks but stays over passes, told to record its new count", shown(result))
    repo.accept({"app/App.tsx": 1450})
    result = repo.stage()
    check(stopped(result, "app/App.tsx", "record 1400"), "a list that is edited must record the count as staged",
          shown(result))


def a_listed_file_within_the_limit_leaves_the_list(repo: Repository) -> None:
    repo.write("scripts/drive.mjs", 1200)
    repo.accept({"scripts/drive.mjs": 1200})
    repo.commit()
    repo.write("scripts/drive.mjs", 900)
    result = repo.stage()
    check(stopped(result, "scripts/drive.mjs", "take it off"), "a listed file within the limit must leave the list",
          shown(result))
    repo.accept({})
    result = repo.stage()
    check(passed(result), "and passes once it has", shown(result))


def a_listed_file_that_is_gone_or_moved_leaves_the_list(repo: Repository) -> None:
    repo.write("src/old.rs", 1100)
    repo.accept({"src/old.rs": 1100})
    repo.commit()
    repo.git("mv", "src/old.rs", "src/new.rs")
    result = repo.run()
    check(stopped(result, "src/old.rs is gone", "src/new.rs is 1100 lines"),
          "a listed file moved away leaves its entry behind, and the new name is over the limit", shown(result))
    repo.accept({"src/new.rs": 1100})
    result = repo.stage()
    check(passed(result), "moving the entry with it passes", shown(result))


def an_entry_must_name_a_file_the_limit_applies_to(repo: Repository) -> None:
    repo.write("src/big.rs", 1100)
    repo.write("notes.md", 2000)
    repo.accept({"src/big.rs": 1100})
    repo.commit()
    repo.accept({"src/big.rs": 1100, "src/missing.rs": 1200, "notes.md": 2000})
    result = repo.stage()
    check(stopped(result, "src/missing.rs, which is not in the repository", "notes.md, which the limit does not"),
          "an entry for a file that is not there, or not source, stops the commit", shown(result))


def files_that_are_not_our_own_source_are_left_alone(repo: Repository) -> None:
    for path in ["vendor/retroarch/menu.c", "work/scratch.py", "scripts/fixtures/replay.js", "desktop/systems.json",
                 "desktop/package-lock.json", "src/table.rs"]:
        repo.write(path, line_limit.LIMIT * 2)
    (repo.root / ".gitattributes").write_text("src/table.rs linguist-generated\n")
    result = repo.stage()
    check(passed(result), "vendor, work, fixtures, data, lockfiles and a file marked generated pass", shown(result))


def what_counts_is_what_is_staged(repo: Repository) -> None:
    repo.write("src/big.rs", 1100)
    repo.git("add", "-A")
    repo.write("src/big.rs", 10)
    result = repo.run()
    check(stopped(result, "src/big.rs is 1100 lines"), "a file staged over the limit stops, trimmed or not on disk",
          shown(result))
    repo.git("add", "-A")
    repo.write("src/big.rs", 1100)
    result = repo.run()
    check(passed(result), "and one staged within it passes, whatever is on disk", shown(result))


CASES = [
    a_new_file_within_the_limit_passes,
    a_new_file_over_the_limit_names_itself_until_it_is_listed,
    a_listed_file_grows_only_by_recording_it,
    a_listed_file_that_shrinks_is_told_to_record_it,
    a_listed_file_within_the_limit_leaves_the_list,
    a_listed_file_that_is_gone_or_moved_leaves_the_list,
    an_entry_must_name_a_file_the_limit_applies_to,
    files_that_are_not_our_own_source_are_left_alone,
    what_counts_is_what_is_staged,
]


def main() -> int:
    for case in CASES:
        repository(case)
    if FAILURES:
        print(f"\n{len(FAILURES)} line limit case(s) failed")
        return 1
    print("\nline limit: every case holds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
