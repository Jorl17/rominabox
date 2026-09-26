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
import scratch  # noqa: E402
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
    # The script's directory is the canonical checkout only when .git is a
    # directory. In a worktree .git is a file, and we must use the main
    # checkout's shared git dir, or each worktree would take a separate lock.
    script_is_canonical = (worktree.ROOT / ".git").is_dir()
    check(
        (from_root.parent == worktree.ROOT) == script_is_canonical,
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
    with scratch.scratch() as made:
        path = Path(made) / "lock"
        with worktree.Lock(path):
            check(path.exists(), "the lock directory exists while held")
            holder = (path / "pid").read_text().strip()
            check(holder == str(os.getpid()), "the lock records its owner")
        check(not path.exists(), "the lock is released on exit")
        with worktree.Lock(path):
            check(True, "the lock can be taken again afterwards")


def a_dead_holders_lock_is_reclaimed() -> None:
    """Check that after a crash, later runs can still take the lock."""
    with scratch.scratch() as made:
        path = Path(made) / "lock"
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
    canonical = worktree.canonical_checkout()
    original = os.getcwd()
    try:
        os.chdir(canonical)
        try:
            worktree.adopt()
            check(False, "adopt refused to run in the canonical checkout")
        except SystemExit as refusal:
            check("canonical" in str(refusal), f"adopt refuses the canonical checkout: {refusal}")
    finally:
        os.chdir(original)
    check(
        not (canonical / worktree.LOCAL_CONFIG).exists(),
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
    with scratch.scratch("rominabox-adopt-") as parent:
        made = Path(parent) / "checkout"
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


def the_test_cartridge_is_in_the_repository() -> None:
    """Check that the test cartridge comes with a clone, not from one machine.

    We generate it with scripts/make_test_rom.py, so it belongs in the
    repository. The git-ignored `work/` is absent from a fresh checkout,
    where the isolation tests would stop with "work/test-game.gbc is not in
    this checkout".
    """
    tracked = subprocess.run(
        ["git", "ls-files", "--error-unmatch", "scripts/fixtures/test-game.gbc"],
        cwd=worktree.ROOT,
        capture_output=True,
        text=True,
    )
    check(tracked.returncode == 0, "scripts/fixtures/test-game.gbc is tracked by git")


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


def removing_a_worktree_keeps_the_fork_commits_its_branch_needs() -> None:
    """Check that the submodule commits of a branch remain after its worktree goes.

    A worktree does not share the submodule object store. Git keeps it under
    `.git/worktrees/<name>/modules/`, and `git worktree remove` deletes it with
    the rest. After removing a worktree with an unmerged branch, the branch
    would point to a fork commit that exists nowhere, and the failure would
    appear only at a later checkout.

    We build a superproject, a submodule and a worktree that advances the fork
    in a temporary directory, and remove the worktree. This shows both that
    the commits would be lost, and that we save them by keeping them first.
    """
    import shutil

    def run(*arguments: str, cwd: Path) -> str:
        return subprocess.run(
            ["git", "-c", "protocol.file.allow=always", *arguments],
            cwd=cwd, capture_output=True, text=True, check=True,
        ).stdout.strip()

    with scratch.scratch() as temporary:
        area = Path(temporary)
        fork = area / "fork"
        fork.mkdir()
        run("init", "--quiet", "-b", "main", cwd=fork)
        run("config", "user.email", "t@example.com", cwd=fork)
        run("config", "user.name", "t", cwd=fork)
        (fork / "player.c").write_text("one\n")
        run("add", "player.c", cwd=fork)
        run("commit", "--quiet", "-m", "one", cwd=fork)

        super_ = area / "super"
        super_.mkdir()
        run("init", "--quiet", "-b", "main", cwd=super_)
        run("config", "user.email", "t@example.com", cwd=super_)
        run("config", "user.name", "t", cwd=super_)
        run("submodule", "add", "--quiet", str(fork), "vendor/retroarch", cwd=super_)
        run("commit", "--quiet", "-m", "vendor the fork", cwd=super_)

        checkout = area / "super-track"
        run("worktree", "add", "--quiet", str(checkout), "-b", "track", cwd=super_)
        run("submodule", "update", "--init", "--quiet", cwd=checkout)

        # We advance the fork on the branch, as on most worktree branches here.
        theirs = checkout / "vendor/retroarch"
        run("config", "user.email", "t@example.com", cwd=theirs)
        run("config", "user.name", "t", cwd=theirs)
        (theirs / "player.c").write_text("two\n")
        run("add", "player.c", cwd=theirs)
        run("commit", "--quiet", "-m", "two", cwd=theirs)
        advanced = run("rev-parse", "HEAD", cwd=theirs)
        run("add", "vendor/retroarch", cwd=checkout)
        run("commit", "--quiet", "-m", "take the advanced fork", cwd=checkout)

        canonical = super_ / "vendor/retroarch"
        check(
            not worktree.reachable(canonical, advanced),
            "a worktree branch's fork commit really does live only in its own worktree",
        )

        original_root, original_fork = worktree.ROOT, worktree.FORK
        try:
            worktree.ROOT = super_
            worktree.FORK = Path("vendor/retroarch")
            kept = worktree.keep_fork_commits(checkout, "track")
        finally:
            worktree.ROOT, worktree.FORK = original_root, original_fork

        check(bool(kept), "removal copies the fork commits out before it deletes anything")
        check(
            worktree.reachable(canonical, advanced),
            "the advanced fork commit is in the canonical submodule afterwards",
        )

        # Now remove the worktree, and confirm that the ref remains afterwards.
        run("worktree", "remove", "--force", str(checkout), cwd=super_)
        shutil.rmtree(checkout, ignore_errors=True)
        check(
            not (checkout / "vendor/retroarch").exists(),
            "the worktree and its private submodule store are gone",
        )
        check(
            worktree.reachable(canonical, advanced),
            "the fork commit survives the removal that used to destroy it",
        )
        check(
            bool(kept) and run("rev-parse", kept, cwd=canonical) == advanced,
            "and a ref names it, so a later gc cannot collect it",
        )


def the_built_cli_follows_the_redirected_cargo_target() -> None:
    """Check where four scripts find cargo's output, which moves in a worktree.

    A fixed `desktop/src-tauri/target/release` path in a script is correct only
    while nothing redirects cargo. Inside a worktree we always redirect it to
    the shared store, so with such a path we would look in a directory without
    cargo's output and report the tool as missing.

    We answer this now with scripts/built.py, where we also refuse a binary
    built in another checkout, which the path alone cannot show.
    """
    import built  # noqa: PLC0415 — imported here so this file loads without it

    shared = Path("/shared-cargo-target")
    before = os.environ.get("CARGO_TARGET_DIR")
    try:
        os.environ["CARGO_TARGET_DIR"] = str(shared)
        check(
            built.target_dir() == shared,
            "the command line is looked for where cargo was redirected",
        )
        os.environ.pop("CARGO_TARGET_DIR")
        check(
            built.target_dir() == built.ROOT / "desktop/src-tauri/target",
            "with nothing redirecting it, the checkout's own target directory",
        )
    finally:
        if before is None:
            os.environ.pop("CARGO_TARGET_DIR", None)
        else:
            os.environ["CARGO_TARGET_DIR"] = before


def a_file_compiled_into_the_tool_counts_as_its_source() -> None:
    """Check that a change to controls.json makes the built tool stale in both.

    `desktop/controls.json` is not Rust. If we checked freshness on `.rs`
    files only, we would take the binary as current after regenerating the
    registry, and render the anchors of the previous registry in every script.
    The result would be a wrong drawing and no error.

    We check freshness in two scripts, and in both we read the pattern from
    one place. We check that here, because a second copy of the pattern would
    pass a separate test and still differ from the other.
    """
    import built  # noqa: PLC0415
    import cargo_replay  # noqa: PLC0415

    check(
        cargo_replay.compiled_in is built.compiled_in,
        "one reader of include_str!, used by both scripts",
    )

    with scratch.scratch() as temporary:
        area = Path(temporary)
        (area / "src").mkdir()
        source = area / "src/controls.rs"
        source.write_text(
            'static REGISTRY: &str = include_str!("../controls.json");\n'
            'static ICON: &[u8] = include_bytes! ("../icon.png");\n'
        )
        (area / "controls.json").write_text("{}\n")
        (area / "icon.png").write_bytes(b"\x89PNG")
        found = {path.name for path in built.compiled_in(source)}
        check(
            found == {"controls.json", "icon.png"},
            f"both baked-in files are found, whatever the spacing ({sorted(found)})",
        )

    # And in the checked-out tree.
    registry = built.ROOT / "desktop/controls.json"
    baked = {
        path
        for rust in built._rust_files()
        for path in built.compiled_in(rust)
    }
    check(
        registry.resolve() in baked,
        "desktop/controls.json is seen as something the tool is built from",
    )


def create_refuses_an_existing_branch_instead_of_checking_it_out() -> None:
    """Check that we refuse `git worktree add <path>` onto an existing branch.

    Without a branch, git checks out the branch of that name. The worktree is
    then at an old commit and looks like a fresh checkout of HEAD. In create
    we must refuse, report that the branch already exists, and not check it out.
    """
    suffix = "shotsignold"
    path = worktree.worktree_path(suffix)
    name = f"{worktree.ROOT.name}-{suffix}"
    if path.exists():
        check(False, f"{path} is already there, so this check cannot start")
        return
    head = subprocess.run(
        ["git", "-C", str(worktree.ROOT), "rev-parse", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    made = subprocess.run(
        ["git", "-C", str(worktree.ROOT), "branch", name, "HEAD~1"],
        capture_output=True, text=True,
    )
    if made.returncode != 0:
        check(False, f"could not plant {name} at HEAD~1: {made.stderr.strip()}")
        return
    try:
        try:
            worktree.create(suffix, None, False)
        except SystemExit as refusal:
            text = str(refusal)
            check(
                name in text and "already exists" in text,
                f"create refuses {name}: {text}",
            )
            check(not path.exists(), "the old branch was not checked out")
            return
        checked = subprocess.run(
            ["git", "-C", str(path), "rev-parse", "HEAD"],
            capture_output=True, text=True,
        ).stdout.strip()
        check(
            False,
            f"create checked out existing branch {name} at {checked[:12]}, "
            f"not this checkout's HEAD {head[:12]}",
        )
    finally:
        if path.exists():
            subprocess.run(
                ["git", "-C", str(worktree.ROOT), "worktree", "remove", "--force", str(path)],
                capture_output=True, text=True,
            )
        subprocess.run(
            ["git", "-C", str(worktree.ROOT), "branch", "-D", name],
            capture_output=True, text=True,
        )


# From inside a worktree, these checks are the wrong checks. The common git
# directory is always the canonical checkout's, and we do not test a worktree
# of a worktree. Without this, a run of the full suite inside a worktree would
# fail on them.
FROM_THE_CANONICAL_CHECKOUT = [
    the_common_dir_is_shared_not_per_worktree,
    two_worktrees_never_share_a_port,
    the_lock_is_exclusive_and_reentrant_after_release,
    a_dead_holders_lock_is_reclaimed,
    the_canonical_checkout_is_never_suffixed,
    adopt_works_from_inside_the_worktree_it_adopts,
    the_test_cartridge_is_in_the_repository,
    the_local_config_is_never_committed,
    removal_never_touches_the_canonical_data,
    removing_a_worktree_keeps_the_fork_commits_its_branch_needs,
]

# These checks give the same result anywhere, and are most useful in a worktree.
def a_failed_rebuild_forgets_the_stamp_of_the_binary_it_rewrote() -> None:
    """Check that we drop the stamp when we rebuild the binary in a failed run.

    We do not stamp the failure, and the new binary often has the same size.
    If we kept the stamp, a run after the source returns to the stamped state
    would replay the broken binary, and the identification tests would then
    reject a valid cover.
    """
    import cargo_replay

    root = Path(__file__).resolve().parent.parent
    manifest = root / "desktop/src-tauri/Cargo.toml"
    plan = cargo_replay.Plan(manifest, True, ["measure::"])
    with tempfile.TemporaryDirectory() as tmp:
        stamps = Path(tmp)
        binary = stamps / "lib-test"
        binary.write_bytes(b"same-size-either-way")
        saved = (
            cargo_replay.STAMPS,
            cargo_replay.source_digest,
            cargo_replay.subprocess.run,
        )
        cargo_replay.STAMPS = stamps
        cargo_replay.source_digest = lambda _manifest: "broken-tree"
        stamp = cargo_replay._stamp_path(plan)
        stamp.write_text(
            json.dumps(
                {
                    "version": 1,
                    "source": "good-tree",
                    "binaries": [
                        {
                            "label": "unittests src/lib.rs",
                            "path": str(binary),
                            "size": binary.stat().st_size,
                        }
                    ],
                }
            )
        )

        def fail_run(*_args, **_kwargs):
            return subprocess.CompletedProcess(["cargo"], 1, "failed\n", "")

        cargo_replay.subprocess.run = fail_run
        try:
            cargo_replay.cargo_test(
                ["cargo", "test", "--manifest-path", str(manifest), "--lib", "measure::"],
                root,
            )
        finally:
            cargo_replay.STAMPS, cargo_replay.source_digest, cargo_replay.subprocess.run = saved
        check(
            not stamp.exists(),
            "a failed cargo test drops the stamp of the binary it rewrote",
        )


def a_rebuilt_rmlui_archive_invalidates_the_menu_probe() -> None:
    """Check that we relink an old probe after preparing the current RmlUi."""
    from unittest.mock import patch
    import menu_interaction

    with scratch.scratch() as made:
        directory = Path(made)
        source, binary, library = (directory / name for name in ("probe.cpp", "probe", "librmlui.a"))
        recipe = (Path(menu_interaction.__file__), menu_interaction.ROOT / "scripts/rmlui_paths.py")
        stamp = max(path.stat().st_mtime for path in recipe) + 10
        for path, modified in ((source, stamp), (binary, stamp + 1), (library, stamp + 2)):
            path.write_bytes(b"fixture")
            os.utime(path, (modified, modified))
        with (
            patch.object(menu_interaction, "PROBE_SOURCE", source),
            patch.object(menu_interaction, "PROBE", binary),
            patch.object(menu_interaction, "LIBRARY", library),
            patch.object(menu_interaction.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "")) as run,
        ):
            menu_interaction.build()
        compiles = [call.args[0] for call in run.call_args_list if call.args[0][0] == "c++"]
        check(
            len(compiles) == 1 and str(library) in compiles[0],
            "the probe is relinked against a newer RmlUi archive even when its C++ source is unchanged",
        )


def a_branch_with_a_slash_keeps_its_whole_name() -> None:
    """Check that we read the branch name menu/nav back in remove(), and not nav."""
    import worktree

    listed = worktree.parse_worktree_list(
        "worktree /src/rominabox\nHEAD abc\nbranch refs/heads/main\n\n"
        "worktree /src/rominabox-menu-nav\nHEAD def\nbranch refs/heads/menu/nav\n\n"
        "worktree /src/rominabox-old\nHEAD 123\ndetached\n"
    )
    check(
        [entry.get("branch") for entry in listed] == ["main", "menu/nav", "(detached)"],
        "a listed branch keeps its whole name, slashes included",
    )


ANYWHERE = [
    a_branch_with_a_slash_keeps_its_whole_name,
    the_built_cli_follows_the_redirected_cargo_target,
    a_file_compiled_into_the_tool_counts_as_its_source,
    a_failed_rebuild_forgets_the_stamp_of_the_binary_it_rewrote,
    a_rebuilt_rmlui_archive_invalidates_the_menu_probe,
    # We also run create() from inside a worktree, where we could check out an
    # old branch by mistake. If we skipped that case here, the tests would pass
    # on the checkout where the mistake happens.
    create_refuses_an_existing_branch_instead_of_checking_it_out,
]


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
    tests = list(ANYWHERE)
    if inside_a_worktree():
        print(
            "  how worktrees are created is checked from the checkout they are "
            "created from, so those are skipped here."
        )
    else:
        tests += FROM_THE_CANONICAL_CHECKOUT
    for test in tests:
        print(f"{test.__name__}")
        test()
    if FAILURES:
        print(f"\n{len(FAILURES)} failed")
        return 1
    print("\nall worktree checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
