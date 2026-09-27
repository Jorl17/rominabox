"""Check that the suite runs without anyone having to remember it.

Without a hook, we would catch a regression only when someone typed the
command for `scripts/test.py`.

In `.githooks/pre-push` we run the whole suite on this machine, which has the
runtime kit prepared, rsvg-convert installed and the offscreen renderer
built. Here we check that the hook is still set up.

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
HOOK = ROOT / ".githooks/pre-push"


def recorded_executable() -> bool:
    """Return whether the hook is recorded in git as a program, because then
    it is executable in every checkout on macOS and Linux."""
    staged = subprocess.run(
        ["git", "ls-files", "--stage", "--", HOOK.relative_to(ROOT).as_posix()],
        cwd=ROOT,
        capture_output=True,
        text=True,
    ).stdout
    return staged.startswith("100755 ")


def runnable_here() -> bool:
    """Return whether the hook file can run as it is with git on this machine."""
    if os.name == "posix":
        return bool(HOOK.stat().st_mode & stat.S_IXUSR)
    if os.name == "nt":
        # With Git for Windows a hook runs whatever its file mode.
        return True
    raise NotImplementedError(f"no hook check declared for os.name {os.name!r}")


def main() -> int:
    failures: list[str] = []

    if not HOOK.exists():
        failures.append(f"{HOOK.name} is gone: nothing runs the suite before a push")
    elif not recorded_executable():
        failures.append(f"{HOOK.name} is not committed as executable, so a checkout's git will not run it")
    elif not runnable_here():
        failures.append(f"{HOOK.name} is not executable, so git will not run it")
    else:
        body = HOOK.read_text().replace('"', " ").split()
        if not {"--all"} <= set(body) or not any(
            word.endswith("scripts/test.py") for word in body
        ):
            failures.append(f"{HOOK.name} no longer runs the whole suite")
        else:
            print("  ok   pre-push hook runs scripts/test.py --all")

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
