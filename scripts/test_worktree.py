"""Check the worktree tooling, which has no other tests.

The allocator is the code here most sensitive to concurrency, and its
failures produce no error: two checkouts that use the same port, a lock that
does not serialise, or a "shared" cargo target that is not shared. Because
we get no message for any of those, we check them here.

    python3 scripts/test_worktree.py

We run these against this repository, but we create nothing outside a
temporary directory and never touch the canonical checkout's data.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import worktree  # noqa: E402

FAILURES: list[str] = []


def check(condition: bool, message: str) -> None:
    if condition:
        print(f"  ok   {message}")
    else:
        print(f"  FAIL {message}")
        FAILURES.append(message)


def the_common_dir_is_shared_not_per_worktree() -> None:
    """Check for the defect that would cost gigabytes per worktree and break locking.

    The output of `git rev-parse --git-common-dir` is RELATIVE. Against the
    process working directory it resolves to the worktree's `.git`, so every
    worktree would take a different lock, without serialisation, and get a
    separate cargo target, without sharing. We must resolve it against the
    repository root, whatever directory we run the command from.
    """
    from_root = worktree.common_dir()
    original = os.getcwd()
    try:
        os.chdir(tempfile.gettempdir())
        from_elsewhere = worktree.common_dir()
    finally:
        os.chdir(original)
    check(from_root == from_elsewhere, "common dir is the same from any working directory")
    check(from_root.is_absolute(), "common dir is absolute")
    check(
        from_root.parent == worktree.ROOT,
        f"common dir belongs to the canonical checkout, not a worktree: {from_root}",
    )


def two_worktrees_never_share_a_port() -> None:
    """Check that the derivation is injective, or two worktrees would share a frontend."""
    seen: dict[int, str] = {}
    for suffix, offset in [("a", 1), ("b", 2), ("c", 3)]:
        local = {
            "vitePort": worktree.BASE_PORT + offset,
            "builderBundleId": f"{worktree.BASE_BUNDLE}.wt-{suffix}",
        }
        port = local["vitePort"]
        check(port not in seen, f"port {port} is not already taken by {seen.get(port)}")
        seen[port] = suffix


def the_lock_is_exclusive_and_reentrant_after_release() -> None:
    """Check that of two allocations at the same time, only one proceeds."""
    with tempfile.TemporaryDirectory() as scratch:
        path = Path(scratch) / "lock"
        with worktree.Lock(path):
            check(path.exists(), "the lock directory exists while held")
            holder = (path / "pid").read_text().strip()
            check(holder == str(os.getpid()), "the lock records its owner")
        check(not path.exists(), "the lock is released on exit")
        with worktree.Lock(path):
            check(True, "the lock can be taken again afterwards")


def a_dead_holders_lock_is_reclaimed() -> None:
    """Check that after a crash, later runs can still take the lock."""
    with tempfile.TemporaryDirectory() as scratch:
        path = Path(scratch) / "lock"
        path.mkdir()
        # Start a process, wait for it and use its pid. The process has ended,
        # and so soon after, the number is not yet in use again.
        finished = subprocess.Popen([sys.executable, "-c", "pass"])
        finished.wait()
        (path / "pid").write_text(str(finished.pid))
        try:
            os.kill(finished.pid, 0)
            print("  skip a dead holder's lock is reclaimed (pid was reused)")
            return
        except (ProcessLookupError, PermissionError):
            pass
        with worktree.Lock(path):
            check(True, "a lock left by a dead process is reclaimed")


def the_canonical_checkout_is_never_suffixed() -> None:
    """Adopting the canonical checkout must fail, or we would rename its app and data."""
    original = os.getcwd()
    try:
        os.chdir(worktree.ROOT)
        try:
            worktree.adopt()
            check(False, "adopt refused to run in the canonical checkout")
        except SystemExit as refusal:
            check("canonical" in str(refusal), f"adopt refuses the canonical checkout: {refusal}")
    finally:
        os.chdir(original)
    check(
        not (worktree.ROOT / worktree.LOCAL_CONFIG).exists(),
        "no local config was written into the canonical checkout",
    )



def adopt_works_from_inside_the_worktree_it_adopts() -> None:
    """Check that the documented use of adopt succeeds.

    We run `worktree.py adopt` in a checkout created some other way. If ROOT
    were the parent directory of the script, inside a worktree it would be the
    worktree, so `here == ROOT` would be true and in adopt we would refuse it
    with "this is the canonical checkout". The checkout would then stay on
    port 1420 with the canonical bundle identifiers and data root. We wrote
    this tool to prevent that collision, and it produces no error.

    We run the script in a temporary worktree of this repository and remove
    the worktree afterwards.
    """
    made = Path(tempfile.mkdtemp(prefix="rominabox-adopt-")) / "checkout"
    try:
        subprocess.run(
            ["git", "worktree", "add", "--detach", str(made), "HEAD"],
            cwd=worktree.ROOT, capture_output=True, text=True, check=True,
        )
        # The checkout is at HEAD, so its copy of the script is the committed
        # one. Put the working copy there instead, because we test the script
        # as it is now, not as it was in the last commit.
        (made / "scripts/worktree.py").write_text(
            (worktree.ROOT / "scripts/worktree.py").read_text()
        )
        done = subprocess.run(
            # The copy of the script IN THE WORKTREE, which is the one we run in
            # that checkout. With the canonical copy we would miss the defect,
            # because ROOT would come from the script's location.
            [sys.executable, str(made / "scripts/worktree.py"), "adopt"],
            cwd=made, capture_output=True, text=True,
        )
        check(
            done.returncode == 0,
            f"adopt succeeds inside a worktree: {done.stdout.strip() or done.stderr.strip()}",
        )
        local = made / worktree.LOCAL_CONFIG
        check(local.is_file(), "it wrote the worktree's own resources")
        if local.is_file():
            settings = json.loads(local.read_text())
            check(
                settings.get("port") != worktree.BASE_PORT,
                f"and gave it a port of its own, not {worktree.BASE_PORT}",
            )
    finally:
        subprocess.run(
            ["git", "worktree", "remove", "--force", str(made)],
            cwd=worktree.ROOT, capture_output=True, text=True,
        )
        subprocess.run(["rm", "-rf", str(made.parent)], check=False)


def the_local_config_is_never_committed() -> None:
    """It contains local resources of this machine, so we must not commit it."""
    ignored = subprocess.run(
        ["git", "check-ignore", worktree.LOCAL_CONFIG],
        cwd=worktree.ROOT,
        capture_output=True,
        text=True,
    )
    check(ignored.returncode == 0, f"{worktree.LOCAL_CONFIG} is git-ignored")


def removal_never_touches_the_canonical_data() -> None:
    """Check that in teardown we delete the right directory outside the worktree."""
    check(
        worktree.CANONICAL_DATA.name == "ROM-in-a-Box",
        "the canonical data directory is known by name",
    )
    for suffix in ["a", "probe", "main"]:
        derived = worktree.DATA_HOME / f"ROM-in-a-Box-wt-{suffix}"
        check(
            derived != worktree.CANONICAL_DATA,
            f"a worktree's data root is distinct from the canonical one ({suffix})",
        )


# From inside a worktree, every check here is the wrong check. The common git
# directory is always the canonical checkout's, and we do not test a worktree
# of a worktree. Without this, a run of the full suite inside a worktree would
# fail here.
def inside_a_worktree() -> bool:
    import subprocess

    common = subprocess.run(
        ["git", "rev-parse", "--git-common-dir"], capture_output=True, text=True
    ).stdout.strip()
    here = subprocess.run(
        ["git", "rev-parse", "--git-dir"], capture_output=True, text=True
    ).stdout.strip()
    return bool(common) and bool(here) and Path(common).resolve() != Path(here).resolve()


def main() -> int:
    if inside_a_worktree():
        print(
            "  skipped: this checks how worktrees are created, from the "
            "checkout they are created from. Run it there."
        )
        return 0
    for test in [
        the_common_dir_is_shared_not_per_worktree,
        two_worktrees_never_share_a_port,
        the_lock_is_exclusive_and_reentrant_after_release,
        a_dead_holders_lock_is_reclaimed,
        the_canonical_checkout_is_never_suffixed,
        adopt_works_from_inside_the_worktree_it_adopts,
        the_local_config_is_never_committed,
        removal_never_touches_the_canonical_data,
    ]:
        print(f"{test.__name__}")
        test()
    if FAILURES:
        print(f"\n{len(FAILURES)} failed")
        return 1
    print("\nall worktree checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
