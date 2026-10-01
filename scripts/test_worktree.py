"""Check the worktree tooling, which has no other tests.

The allocator is the code here most sensitive to concurrency, and its
failures produce no error: two checkouts that use the same port, a lock that
does not serialise, or two checkouts that build into one cargo target.
Because we get no message for any of those, we check them here.

    python3 scripts/test_worktree.py

We run these against this repository, but we create nothing outside a
temporary directory and never touch the canonical checkout's data.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import processes  # noqa: E402
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
    """Check for the defect that would break locking entirely.

    The output of `git rev-parse --git-common-dir` is RELATIVE. Against the
    process working directory it resolves to the worktree's `.git`, so every
    worktree would take a different lock. We must resolve it against the
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
        if processes.alive(finished.pid):
            print("  skip a dead holder's lock is reclaimed (pid was reused)")
            return
        with worktree.Lock(path):
            check(True, "a lock left by a dead process is reclaimed")


def a_live_holders_lock_is_kept() -> None:
    """Check that we never take a lock from a process that is still running."""
    with scratch.scratch() as made:
        path = Path(made) / "lock"
        path.mkdir()
        (path / "pid").write_text(str(os.getpid()))
        check(not worktree.Lock(path)._stale(), "a lock held by a running process is not stale")
        check((path / "pid").is_file(), "the running holder's lock is left in place")


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
            # The checkout is at HEAD, so its scripts are the committed ones. Put
            # the working copies there instead, of the script and every module
            # it may import, because we test the script as it is now, not as it
            # was in the last commit.
            for script in (worktree.ROOT / "scripts").glob("*.py"):
                (made / "scripts" / script.name).write_bytes(script.read_bytes())
            done = subprocess.run(
                # The copy of the script IN THE WORKTREE, which is the one we run in
                # that checkout. With the canonical copy we would miss the defect,
                # because ROOT would come from the script's location.
                [sys.executable, str(made / "scripts/worktree.py"), "adopt"],
                cwd=made, capture_output=True, text=True,
            )
            if shutil.disk_usage(made).free < worktree.MINIMUM_FREE_BYTES:
                check("15 GB" in done.stderr, f"adopt refuses on a nearly full disk: {done.stderr.strip()}")
                return
            check(
                done.returncode == 0,
                f"adopt succeeds inside a worktree: {done.stdout.strip() or done.stderr.strip()}",
            )
            local = made / worktree.LOCAL_CONFIG
            check(local.is_file(), "it wrote the worktree's own resources")
            if local.is_file():
                settings = json.loads(local.read_text())
                port = settings.get("vitePort")
                check(
                    port not in (None, worktree.BASE_PORT),
                    f"and gave it a port of its own, {port}" if port not in (None, worktree.BASE_PORT)
                    else f"and gave it the canonical port {worktree.BASE_PORT}, or none: {port}",
                )
            check((made / "desktop/src-tauri/resources/bin").is_dir() or
                  not (worktree.ROOT / "desktop/src-tauri/resources/bin").exists(),
                  "and gave it the build output create gives a worktree")
            # In the environment documented for the worktree, cargo builds
            # inside the worktree. With a target shared between checkouts, one
            # could run another's build, because the freshness check in cargo
            # uses file times and paths relative to the checkout. We start the
            # shell with a shared target already set in its environment.
            asked = subprocess.run(
                ["bash", "-c",
                 f'eval "$("{sys.executable}" scripts/worktree.py env)" && '
                 "cargo metadata --no-deps --offline --format-version 1 "
                 "--manifest-path desktop/crates/rominabox-scratch/Cargo.toml"],
                cwd=made, capture_output=True, text=True,
                env={**os.environ,
                     "CARGO_TARGET_DIR": str(worktree.common_dir() / "shared-cargo-target")},
            )
            target = (Path(json.loads(asked.stdout)["target_directory"])
                      if asked.returncode == 0 else None)
            check(
                target is not None and target.resolve().is_relative_to(made.resolve()),
                f"and cargo builds it into its own target: {target or asked.stderr.strip()}",
            )
        finally:
            # As in remove: we unlink a link and never follow it.
            worktree.unlink_artifacts(made)
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


def removal_deletes_only_the_worktrees_own_accounts_folder() -> None:
    """Check that in teardown we delete only this worktree's folder outside it."""
    home = worktree.player_support.user_data()
    cases = [
        ({"suffix": "a", "accountsFolder": "ROM-in-a-Box Accounts-wt-a"}, home / "ROM-in-a-Box Accounts-wt-a"),
        ({"suffix": "a", "accountsFolder": "ROM-in-a-Box Accounts"}, None),
        ({"suffix": "a", "accountsFolder": "ROM-in-a-Box Accounts-wt-b"}, None),
        ({"suffix": "a", "accountsFolder": "../ROM-in-a-Box Accounts-wt-a"}, None),
        ({"suffix": "a", "accountsFolder": "elsewhere\\ROM-in-a-Box Accounts-wt-a"}, None),
        ({"suffix": "a"}, None),
        ({"accountsFolder": "ROM-in-a-Box Accounts-wt-"}, None),
    ]
    for local, wanted in cases:
        got = worktree.own_accounts(local)
        check(got == wanted, f"removal with {local} deletes {wanted}" if got == wanted
              else f"removal with {local} would delete {got}, not {wanted}")


def shared_directories_are_linked_and_removal_never_follows_them() -> None:
    """We link into a worktree what we only read there and copy what a build
    writes, and on removal we unlink the links and leave their targets alone.
    On Windows the link is a junction, because a symbolic link there requires
    a privilege that most accounts lack (WinError 1314)."""
    with tempfile.TemporaryDirectory() as directory:
        canonical = Path(directory) / "canonical"
        tree = Path(directory) / "tree"
        tree.mkdir()
        for relative in worktree.COPIED_ARTIFACTS + worktree.SHARED_ARTIFACTS:
            (canonical / relative).mkdir(parents=True)
            (canonical / relative / "kept.txt").write_text(relative.as_posix(), encoding="utf-8")
        worktree.link_build_artifacts(tree, own_copy=False, canonical=canonical)
        for relative in worktree.SHARED_ARTIFACTS:
            check(worktree.redirected(tree / relative) and (tree / relative / "kept.txt").is_file(),
                  f"{relative.as_posix()} is linked and read through the link")
        for relative in worktree.COPIED_ARTIFACTS:
            check(not worktree.redirected(tree / relative) and (tree / relative / "kept.txt").is_file(),
                  f"{relative.as_posix()} is a copy a build may write")
        worktree.unlink_artifacts(tree)
        for relative in worktree.SHARED_ARTIFACTS:
            check(not os.path.lexists(tree / relative), f"{relative.as_posix()}'s link is gone")
        for relative in worktree.COPIED_ARTIFACTS + worktree.SHARED_ARTIFACTS:
            check((canonical / relative / "kept.txt").is_file(),
                  f"the canonical {relative.as_posix()} is untouched")


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
    """Check where four scripts find cargo's output, which CARGO_TARGET_DIR moves.

    A fixed `desktop/src-tauri/target/release` path in a script is correct only
    while nothing redirects cargo. With a redirect, we would look in a directory
    without cargo's output and report the tool as missing. We resolve the
    location with scripts/built.py.
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


def create_refuses_without_room_for_a_cargo_target() -> None:
    """Each worktree has a separate cargo target, so in create we require 15 GB
    free and refuse before we make anything. We fake only the disk space."""
    import types

    suffix = "roomcheck"
    path = worktree.worktree_path(suffix)
    name = f"{worktree.ROOT.name}-{suffix}"
    if path.exists():
        check(False, f"{path} is already there, so this check cannot start")
        return
    measure = shutil.disk_usage
    shutil.disk_usage = lambda _: types.SimpleNamespace(free=worktree.MINIMUM_FREE_BYTES - 1)
    try:
        try:
            worktree.create(suffix, None, False)
            refused = ""
        except SystemExit as refusal:
            refused = str(refusal)
        check("15 GB" in refused, f"create refuses on a nearly full disk: {refused or 'it went ahead'}")
        check(not path.exists() and not worktree.branch_exists(name), "and made nothing")
    finally:
        shutil.disk_usage = measure
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
    a_live_holders_lock_is_kept,
    the_canonical_checkout_is_never_suffixed,
    adopt_works_from_inside_the_worktree_it_adopts,
    the_test_cartridge_is_in_the_repository,
    the_local_config_is_never_committed,
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


def build_probe(directory: Path, oldest_first: tuple[str, ...]) -> list[list[str]]:
    """Return the compiles in menu_interaction.build() for a probe in `directory`
    whose source, library and program were last written in the given order.
    The program is the file written by the compiler for the probe's name."""
    from unittest.mock import patch
    import menu_interaction
    import toolchain

    named = directory / "probe"
    files = {"source": directory / "probe.cpp", "library": directory / "librmlui.a",
             "program": toolchain.executable(named)}
    recipe = (Path(menu_interaction.__file__), menu_interaction.ROOT / "scripts/rmlui_paths.py")
    stamp = max(path.stat().st_mtime for path in recipe) + 10
    for offset, role in enumerate(oldest_first):
        files[role].write_bytes(b"fixture")
        os.utime(files[role], (stamp + offset, stamp + offset))
    with (
        patch.object(menu_interaction, "PROBE_SOURCE", files["source"]),
        patch.object(menu_interaction, "PROBE", named),
        patch.object(menu_interaction, "LIBRARY", files["library"]),
        patch.object(menu_interaction.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "")) as run,
    ):
        menu_interaction.build()
    return [call.args[0] for call in run.call_args_list if call.args[0][0] == "c++"]


def a_rebuilt_rmlui_archive_invalidates_the_menu_probe() -> None:
    """Check that we relink an old probe after preparing the current RmlUi."""
    with scratch.scratch() as made:
        directory = Path(made)
        compiles = build_probe(directory, ("source", "program", "library"))
        check(
            len(compiles) == 1 and str(directory / "librmlui.a") in compiles[0],
            "the probe is relinked against a newer RmlUi archive even when its C++ source is unchanged",
        )
        # A fresh checkout has no work/probe, so in build() we must not put the
        # lock there and stop with FileNotFoundError.
        check(
            any(path.suffix == ".lock" for path in directory.iterdir()),
            "a probe built somewhere else takes its lock with it, so a checkout where nothing has built the probe builds it",
        )


def an_up_to_date_menu_probe_is_not_compiled_again() -> None:
    """The compiler output has the platform's program suffix, for example
    rml_probe.exe on Windows. Without the suffix we would find nothing there
    and compile the probe again on every run on Windows."""
    with scratch.scratch() as made:
        compiles = build_probe(Path(made), ("source", "library", "program"))
        check(not compiles, "a probe newer than everything it is built from is not compiled again")


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
    removal_deletes_only_the_worktrees_own_accounts_folder,
    shared_directories_are_linked_and_removal_never_follows_them,
    a_branch_with_a_slash_keeps_its_whole_name,
    the_built_cli_follows_the_redirected_cargo_target,
    a_file_compiled_into_the_tool_counts_as_its_source,
    a_failed_rebuild_forgets_the_stamp_of_the_binary_it_rewrote,
    a_rebuilt_rmlui_archive_invalidates_the_menu_probe,
    an_up_to_date_menu_probe_is_not_compiled_again,
    # We also run create() from inside a worktree, where we could check out an
    # old branch by mistake. If we skipped that case here, the tests would pass
    # on the checkout where the mistake happens.
    create_refuses_an_existing_branch_instead_of_checking_it_out,
    create_refuses_without_room_for_a_cargo_target,
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
