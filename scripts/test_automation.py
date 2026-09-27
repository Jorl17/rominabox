"""Check that the suite runs without anyone having to remember it.

Without a hook, we would catch a regression only when someone typed the
command for `scripts/test.py`.

In `.githooks/pre-push` we run the whole suite on this machine, which has the
runtime kit prepared, rsvg-convert installed and the offscreen renderer
built. In `.githooks/pre-commit` we check the line limit
(scripts/line_limit.py). Here we check that both hooks are still set up.

We do not check that the hook is installed in a given checkout.
`core.hooksPath` is local configuration and is unset in a fresh clone, so
here we check the files in the repository, and in the hook itself we report
its installation.
"""

from __future__ import annotations

import os
import stat
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PRE_PUSH = ROOT / ".githooks/pre-push"
PRE_COMMIT = ROOT / ".githooks/pre-commit"


def recorded_executable(hook: Path) -> bool:
    """Return whether the hook is recorded in git as a program, because then
    it is executable in every checkout on macOS and Linux."""
    staged = subprocess.run(
        ["git", "ls-files", "--stage", "--", hook.relative_to(ROOT).as_posix()],
        cwd=ROOT,
        capture_output=True,
        text=True,
    ).stdout
    return staged.startswith("100755 ")


def runnable_here(hook: Path) -> bool:
    """Return whether the hook file can run as it is with git on this machine."""
    if os.name == "posix":
        return bool(hook.stat().st_mode & stat.S_IXUSR)
    if os.name == "nt":
        # With Git for Windows a hook runs whatever its file mode.
        return True
    raise NotImplementedError(f"no hook check declared for os.name {os.name!r}")


def wired(hook: Path) -> str | None:
    """Return what is wrong with how `hook` is stored, or None."""
    if not hook.exists():
        return f"{hook.name} is gone"
    if not recorded_executable(hook):
        return f"{hook.name} is not committed as executable, so a checkout's git will not run it"
    if not runnable_here(hook):
        return f"{hook.name} is not executable, so git will not run it"
    return None


def main() -> int:
    failures: list[str] = []

    for hook, script, extra, runs in [
        (PRE_PUSH, "scripts/test.py", {"--all"}, "the whole suite"),
        (PRE_COMMIT, "scripts/line_limit.py", set(), "the line limit"),
    ]:
        problem = wired(hook)
        if problem:
            failures.append(f"{problem}: nothing runs {runs}")
            continue
        body = hook.read_text().replace('"', " ").split()
        if not extra <= set(body) or not any(word.endswith(script) for word in body):
            failures.append(f"{hook.name} no longer runs {runs}")
        else:
            print(f"  ok   {hook.name} hook runs {' '.join([script, *sorted(extra)])}")

    configured = subprocess.run(
        ["git", "config", "--get", "core.hooksPath"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if configured == ".githooks":
        print("  ok   this checkout has core.hooksPath set to .githooks")
    else:
        # This is not a failure, because it is unset in a fresh clone by design.
        print(
            f"  note this checkout does not use the hook "
            f"(core.hooksPath is {configured or 'unset'}); install it with\n"
            "         git config core.hooksPath .githooks"
        )

    if failures:
        for failure in failures:
            print(f"  FAIL {failure}", file=sys.stderr)
        print(
            f"\n{len(failures)} piece(s) of the automation are unwired. The suite "
            "is only worth having if something other than a person runs it.",
            file=sys.stderr,
        )
        return 1
    print("\nthe suite runs without being remembered")
    return 0


if __name__ == "__main__":
    sys.exit(main())
