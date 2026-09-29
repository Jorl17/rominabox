"""Create, adopt and remove isolated worktrees.

Two checkouts share more than is apparent: a dev-server port pinned with
`strictPort`, the builder's identifier and the dev server for it, the
identities of the exported games (and with them their data and sandboxes),
the QUICK SIGN IN accounts folder, and a cargo target. None of those
collisions produces an error. Each one produces a plausible wrong answer,
the worst being a screenshot of a build from the other checkout.

    python3 scripts/worktree.py create feature-x     # make one, in ../rominabox-worktrees/
    python3 scripts/worktree.py adopt               # from inside an existing one
    python3 scripts/worktree.py list                # what exists, and its resources
    python3 scripts/worktree.py remove feature-x    # and clean up after it

Isolation is opt-in. With no `worktree.local.json` in a checkout, nothing
changes: the port is 1420, the identifiers have no suffix, and
`stable_identity` is the same. This is important, because a regression test
checks it so that a player's saves survive a re-export of the same game.

Each worktree has a separate cargo target. With a shared one, a worktree
could run the build of another. The freshness check in cargo uses file times
and paths relative to the checkout, so a build from another checkout passes
as up to date. A target takes several gigabytes, so in create and adopt we
stop with an error when less than 15 GB is free. We share the submodule's
object store, which git does at no cost.

We copy the prepared runtime kit and do not share it. Through a symlink,
staging a design in a worktree would write into the checkout it came from,
and every worktree linked to it would then have a kit that does not match
its design.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

from directory_links import link_directory, redirected
import player_support
import processes

def _canonical() -> Path:
    """Return the main checkout, whatever checkout we run this script from.

    Inside a worktree the script's parent directory is the worktree. With it,
    in `adopt`, which we run from inside a worktree, we would take the
    worktree for the canonical checkout and refuse it. The checkout would then
    stay on port 1420 with the canonical bundle identifiers and data root. We
    wrote this tool to prevent that collision, and it produces no error.

    The common git directory is the main checkout's `.git` in every worktree,
    so its parent is the main checkout.
    """
    here = Path(__file__).resolve().parent.parent
    try:
        common = subprocess.run(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            cwd=here, capture_output=True, text=True, check=True,
        ).stdout.strip()
    except (subprocess.CalledProcessError, OSError):
        return here
    return Path(common).resolve().parent if common else here


ROOT = _canonical()
BASE_PORT = 1420
BASE_BUNDLE = "com.rominabox.desktop"
LOCAL_CONFIG = "worktree.local.json"

# We allocate ports one offset at a time and store the offset in git's
# per-worktree metadata, never in a committed file, because it is local state
# and `git worktree remove` deletes it with everything else.
OFFSET_FILE = "rominabox_port_offset"
LOCK_NAME = "rominabox-worktree-lock"


def worktree_path(suffix: str) -> Path:
    """Return the path of the worktree for `suffix`. We keep every worktree in
    one folder beside the checkout, listed in .claude/settings.json as an
    additional working directory."""
    return ROOT.parent / f"{ROOT.name}-worktrees" / suffix
MAX_OFFSET = 200
# The minimum free space, because each worktree has a separate cargo target.
MINIMUM_FREE_BYTES = 15 * 10**9


def require_room(path: Path) -> None:
    """Refuse to set up a worktree on a disk with less than 15 GB free."""
    existing = next(folder for folder in (path, *path.parents) if folder.exists())
    free = shutil.disk_usage(existing).free
    if free < MINIMUM_FREE_BYTES:
        raise SystemExit(
            f"only {free // 10**8 / 10} GB free; a worktree needs "
            f"{MINIMUM_FREE_BYTES // 10**9} GB for its own cargo target"
        )


def git(*arguments: str, cwd: Path | None = None) -> str:
    result = subprocess.run(
        ["git", *arguments],
        cwd=cwd or ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip()


def common_dir() -> Path:
    """Return the repository's shared git directory, not a worktree's private one.

    The output of `--git-common-dir` is relative. Resolved against the process
    working directory, it is the *worktree's* `.git` when we run the command
    from inside one. Every worktree would then get a separate lock, so
    allocation would not serialise.
    """
    reported = Path(git("rev-parse", "--git-common-dir"))
    return reported if reported.is_absolute() else (ROOT / reported).resolve()


def worktrees() -> list[dict]:
    """Return every checkout known to git, with its suffix and resources."""
    entries = parse_worktree_list(git("worktree", "list", "--porcelain"))
    for entry in entries:
        config = entry["path"] / LOCAL_CONFIG
        entry["local"] = json.loads(config.read_text()) if config.exists() else None
    return entries


def parse_worktree_list(porcelain: str) -> list[dict]:
    """Return the paths and branches from `git worktree list --porcelain`."""
    entries: list[dict] = []
    current: dict = {}
    for line in porcelain.splitlines():
        if not line:
            if current:
                entries.append(current)
            current = {}
            continue
        key, _, value = line.partition(" ")
        if key == "worktree":
            current = {"path": Path(value)}
        elif key == "branch":
            current["branch"] = value.removeprefix("refs/heads/")
        elif key == "detached":
            current["branch"] = "(detached)"
    if current:
        entries.append(current)
    return entries


def port_is_free(port: int) -> bool:
    """Return whether anything listens on the port, not only whether we reserved it.

    We keep the reservation of a stopped sibling worktree, but an orphaned
    process or an unrelated application can use a port without any
    reservation, and we can learn about those only from the system.
    """
    if sys.platform == "darwin" or sys.platform.startswith("linux"):
        probe = subprocess.run(
            ["lsof", "-i", f":{port}", "-sTCP:LISTEN"],
            capture_output=True,
            text=True,
        )
        return probe.returncode != 0 or not probe.stdout.strip()
    if sys.platform == "win32":
        # Windows has no lsof, and the words in netstat output are translated. A
        # dev server listens on the loopback addresses, so we try to connect.
        import socket

        for family, address in ((socket.AF_INET, "127.0.0.1"), (socket.AF_INET6, "::1")):
            with socket.socket(family, socket.SOCK_STREAM) as probe:
                probe.settimeout(0.5)
                if probe.connect_ex((address, port)) == 0:
                    return False
        return True
    raise NotImplementedError(f"no way to ask whether a port is taken on {sys.platform}")


class Lock:
    """Serialise allocation with a directory, which is created atomically.

    Two `create` runs at the same time must not choose the same offset. `mkdir`
    is the portable atomic primitive, and `flock` is not always dependable.
    """

    def __init__(self, path: Path):
        self.path = path
        self.held = False

    def __enter__(self) -> "Lock":
        deadline = time.monotonic() + 30
        while True:
            try:
                self.path.mkdir(parents=True)
                (self.path / "pid").write_text(str(os.getpid()))
                self.held = True
                return self
            except FileExistsError:
                if self._stale():
                    continue
                if time.monotonic() > deadline:
                    raise SystemExit(
                        f"another worktree command has held {self.path} for 30s"
                    )
                time.sleep(0.2)

    def _stale(self) -> bool:
        """Reclaim a lock whose owner process has ended, without taking one in use."""
        pid_file = self.path / "pid"
        try:
            holder = int(pid_file.read_text().strip())
        except (OSError, ValueError):
            return False
        if processes.alive(holder):
            return False
        # We move it aside and check again, so we never delete a lock that a
        # new owner has just taken.
        aside = self.path.with_suffix(f".stale.{os.getpid()}")
        try:
            self.path.rename(aside)
        except OSError:
            return False
        try:
            if int((aside / "pid").read_text().strip()) != holder:
                aside.rename(self.path)
                return False
        except (OSError, ValueError):
            pass
        shutil.rmtree(aside, ignore_errors=True)
        return True

    def __exit__(self, *_: object) -> None:
        if self.held:
            shutil.rmtree(self.path, ignore_errors=True)


def reserved_offsets() -> set[int]:
    return {
        entry["local"]["portOffset"]
        for entry in worktrees()
        if entry.get("local") and "portOffset" in entry["local"]
    }


def allocate_offset(existing: int | None) -> int:
    """Keep a valid offset, and otherwise take the lowest free one.

    Running setup again on an active worktree must not move its port, so we
    keep an offset that is already ours without the liveness check, because
    the server on that port is ours.
    """
    if existing is not None and existing not in reserved_offsets() - {existing}:
        return existing
    taken = reserved_offsets()
    for offset in range(1, MAX_OFFSET + 1):
        if offset in taken:
            continue
        if port_is_free(BASE_PORT + offset):
            return offset
    raise SystemExit(f"no free port offset below {MAX_OFFSET}")


def write_local(path: Path, suffix: str, offset: int) -> dict:
    """Write the one file read by every other tool, so we derive no name twice."""
    local = {
        "suffix": suffix,
        "portOffset": offset,
        "vitePort": BASE_PORT + offset,
        "builderBundleId": f"{BASE_BUNDLE}.wt-{suffix}",
        # We put this prefix in an exported game's identity, and so in its data
        # folder and its sandbox (packaging.rs).
        "gameBundlePrefix": f"app.rominabox.game.wt-{suffix}",
        # The shared QUICK SIGN IN folder for this worktree's exports, in the
        # per-user application data folder like the player's folder. We use it
        # in the exporter.
        "accountsFolder": f"ROM-in-a-Box Accounts-wt-{suffix}",
    }
    (path / LOCAL_CONFIG).write_text(json.dumps(local, indent=2) + "\n")
    return local


# The prepared runtime kit (the frozen RetroArch player, the cores, the menu
# assets) is build output and is not in git. Without it we cannot export or
# launch anything in a worktree, and a RetroArch build per worktree is too slow.
#
# We copy the kit and do not share it with a symlink. Staging a design writes
# into the kit, and we stage designs in several scripts (for example, we build
# a kit to photograph in menu_shots.py), so through a shared link one worktree
# would overwrite the canonical kit and every other worktree would have a kit
# that does not match its design.
COPIED_ARTIFACTS = [
    Path("desktop/src-tauri/resources/runtime"),
    # The same applies to these small files. In build_kit.py we install the
    # preview renderer into resources/preview, and in build_builder.py we write
    # the command line into resources/bin, so with links we would write into
    # the canonical checkout.
    Path("desktop/src-tauri/resources/preview"),
    Path("desktop/src-tauri/resources/bin"),
]

# We only ever read these, so we share them at no cost and save a lot of
# space, because node_modules alone is larger than the kit.
SHARED_ARTIFACTS = [
    # Without node_modules we cannot run the frontend tests in a worktree, and
    # the failure, "tsc: command not found", looks like a fault in the change.
    # It is build output, the same in every worktree, and not in git.
    Path("desktop/node_modules"),
    # The catalogues and picture lists for the identification measurement. We
    # fetch them on purpose and never during a test, so without them we cannot
    # run those tests in a worktree. The error message suggests a fetch, and in
    # every checkout that would mean many requests to another party's API for
    # the same files.
    Path("work/identification-cache"),
]


def accounts_of(local: dict) -> Path | str:
    """Return the folder for the QUICK SIGN IN accounts of this worktree's exports."""
    folder = local.get("accountsFolder", "")
    return player_support.user_data() / folder if folder else "(none)"


def link_build_artifacts(path: Path, own_copy: bool, canonical: Path = ROOT) -> None:
    """The build output from `canonical` for a worktree: copied where we write it
    in a build, and linked where we only read it."""
    for relative in COPIED_ARTIFACTS + SHARED_ARTIFACTS:
        copy = own_copy or relative in COPIED_ARTIFACTS
        source = canonical / relative
        if not source.exists():
            print(f"  {relative} is not prepared here; skipping")
            continue
        target = path / relative
        if target.exists() or redirected(target):
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        if copy:
            started = time.monotonic()
            shutil.copytree(source, target, symlinks=True)
            print(f"  copied {relative} ({time.monotonic() - started:.0f}s)")
        else:
            link_directory(target, source)
            print(f"  linked {relative} -> the canonical checkout")

def describe(local: dict) -> str:
    return (
        f"  port        {local['vitePort']}\n"
        f"  builder id  {local['builderBundleId']}\n"
        f"  game ids    {local['gameBundlePrefix']}.*\n"
        f"  accounts    {accounts_of(local)}"
    )


def branch_exists(name: str) -> bool:
    return subprocess.run(
        ["git", "show-ref", "--verify", "--quiet", f"refs/heads/{name}"],
        cwd=ROOT,
    ).returncode == 0


def create(suffix: str, branch: str | None, own_runtime: bool) -> int:
    path = worktree_path(suffix)
    if path.exists():
        raise SystemExit(f"{path} already exists; use adopt, or pick another suffix")
    require_room(path)
    path.parent.mkdir(exist_ok=True)
    name = branch or f"{ROOT.name}-{suffix}"
    # With `git worktree add <path>` and no -b, git checks out the branch named
    # after the directory when one exists. The worktree can then be at an old
    # commit that looks like a fresh checkout of HEAD.
    if branch_exists(name):
        raise SystemExit(
            f"branch {name} already exists, so it was not checked out. "
            "create always starts a new branch from this checkout's HEAD. "
            "Pass a new --branch, or delete the old branch first."
        )
    print(git("worktree", "add", "-b", name, str(path), "HEAD"))

    # `git worktree add` does not populate submodules, and the worktree is
    # useless without the RetroArch fork. With --reference we share the object
    # store with the existing checkout, so only the working files are new.
    print("checking out submodules (objects shared with this checkout)...")
    started = time.monotonic()
    subprocess.run(
        ["git", "submodule", "update", "--init", "--reference", str(ROOT / "vendor/retroarch")],
        cwd=path,
        check=True,
    )
    print(f"  took {time.monotonic() - started:.0f}s")

    link_build_artifacts(path, own_runtime)

    with Lock(common_dir() / LOCK_NAME):
        offset = allocate_offset(None)
        local = write_local(path, suffix, offset)
    print(f"\n{path}\n{describe(local)}")
    print(f"\n  cd {path} && eval \"$(python3 scripts/worktree.py env)\"")
    return 0


def canonical_checkout() -> Path:
    """Return the main checkout. A worktree's copy of this script is elsewhere,
    so the script's directory is not the main checkout."""
    return common_dir().parent


def adopt() -> int:
    """Set up a worktree created by something else, such as a bare git command."""
    here = Path.cwd().resolve()
    known = {entry["path"].resolve(): entry for entry in worktrees()}
    if here not in known:
        raise SystemExit(f"{here} is not a git worktree of this repository")
    if here == canonical_checkout():
        raise SystemExit("this is the canonical checkout; it is never suffixed")
    suffix = (here.name if here.parent == worktree_path(here.name).parent
              else here.name.removeprefix(f"{ROOT.name}-") or here.name)
    existing = known[here].get("local") or {}
    require_room(here)
    # The setup from create, for a worktree created some other way.
    link_build_artifacts(here, own_copy=False)
    with Lock(common_dir() / LOCK_NAME):
        offset = allocate_offset(existing.get("portOffset"))
        local = write_local(here, suffix, offset)
    print(f"{here}\n{describe(local)}")
    return 0

def environment() -> int:
    """Print the shell exports for a worktree, for `eval`."""
    config = Path.cwd().resolve() / LOCAL_CONFIG
    if not config.exists():
        raise SystemExit(f"no {LOCAL_CONFIG} here; run adopt first")
    local = json.loads(config.read_text())
    print(f"export ROMINABOX_VITE_PORT={local['vitePort']}")
    print(f"export ROMINABOX_BUNDLE_ID={local['builderBundleId']}")
    print(f"export ROMINABOX_GAME_BUNDLE_PREFIX={local['gameBundlePrefix']}")
    if local.get("accountsFolder"):
        print(f"export ROMINABOX_ACCOUNTS_FOLDER={json.dumps(local['accountsFolder'])}")
    # A separate cargo target, beside the manifests. A shell set up for the
    # shared target may still have it in its environment.
    print("unset CARGO_TARGET_DIR")
    return 0


def show() -> int:
    for entry in worktrees():
        local = entry.get("local")
        marker = "canonical" if entry["path"].resolve() == canonical_checkout() else "worktree"
        print(f"{entry['path']}  [{entry.get('branch', '?')}]  {marker}")
        print(describe(local) if local else "  (not set up — run adopt inside it)")
    return 0


def own_accounts(local: dict) -> Path | None:
    """Return the accounts folder that we may delete in `remove`: the one of this
    worktree, named for its suffix, directly in the per-user data folder. Never
    the player's, and never a name that leads anywhere else."""
    folder = local.get("accountsFolder", "")
    suffix = local.get("suffix", "")
    if not folder or not suffix or "/" in folder or "\\" in folder or not folder.endswith(f"-wt-{suffix}"):
        return None
    return player_support.user_data() / folder


def unlink_artifacts(path: Path) -> None:
    """A shared artifact is a link into the canonical checkout, which we unlink
    and never follow when we remove the worktree. An older worktree may still
    have a link to an artifact that we now copy, so we check every one."""
    for relative in COPIED_ARTIFACTS + SHARED_ARTIFACTS:
        link = path / relative
        if redirected(link):
            link.unlink()


def remove(suffix: str, keep_data: bool) -> int:
    path = worktree_path(suffix)
    entry = next((e for e in worktrees() if e["path"].resolve() == path.resolve()), None)
    if entry is None:
        raise SystemExit(f"no worktree at {path}")
    local = entry.get("local")

    # The accounts folder is outside the worktree, so `git worktree remove`
    # does not delete it, and we delete it by hand in teardown.
    accounts = own_accounts(local) if local and not keep_data else None
    if accounts is not None and accounts.is_dir() and not redirected(accounts):
        shutil.rmtree(accounts)
        print(f"removed {accounts}")
    kept = keep_fork_commits(path, entry.get("branch"))
    unlink_artifacts(path)
    git("worktree", "remove", "--force", str(path))
    print(f"removed {path}")
    if kept:
        print(f"kept its fork commits at {kept}")
    print("shared folders were unlinked and left alone")
    return 0


FORK = Path("vendor/retroarch")
KEPT_REFS = "refs/rominabox/kept"


def keep_fork_commits(path: Path, branch: str | None) -> str | None:
    """Copy a worktree's fork commits into the canonical submodule before it goes.

    A worktree does not share the submodule's object store. Git puts it under
    `.git/worktrees/<name>/modules/`, and `git worktree remove` deletes that
    directory with everything else in it. Removing the worktree of an unmerged
    branch would destroy the only copy of the fork commits of the branch. The
    branch would remain, with a submodule commit that exists nowhere, and the
    failure would appear at checkout time and not at removal time.

    We fetch them under a separate ref, so that `git gc` cannot collect them.
    Return the ref, or None when the worktree has no fork to keep.
    """
    if not (path / FORK).exists():
        return None
    canonical = ROOT / FORK
    if not canonical.is_dir():
        return None
    name = f"{KEPT_REFS}/{path.name}"
    try:
        git("fetch", "--quiet", str(path / FORK), f"+HEAD:{name}", cwd=canonical)
    except subprocess.CalledProcessError as error:
        raise SystemExit(
            f"could not copy {path.name}'s fork commits out of its worktree, "
            "and removing it would destroy them:\n"
            f"{(error.stderr or '').strip()}"
        )
    # HEAD is where that checkout is now. A merge uses the pointer recorded on
    # the branch, and the two differ on a branch where we committed the
    # superproject and then moved the submodule on.
    if branch and branch != "(detached)":
        recorded = git("rev-parse", f"{branch}:{FORK.as_posix()}")
        if not reachable(canonical, recorded):
            git("fetch", "--quiet", str(path / FORK), f"+{recorded}:{name}-recorded",
                cwd=canonical)
    return name


def reachable(repository: Path, commit: str) -> bool:
    return subprocess.run(
        ["git", "cat-file", "-e", f"{commit}^{{commit}}"],
        cwd=repository, capture_output=True,
    ).returncode == 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    made = commands.add_parser("create", help="make an isolated worktree")
    made.add_argument("suffix")
    made.add_argument(
        "--branch",
        help="new branch to create from this checkout's HEAD; default is the directory name",
    )
    made.add_argument(
        "--own-runtime",
        action="store_true",
        help="copy the 164 MB runtime kit instead of sharing it; needed when changing the fork OR a design, because staging writes into the kit",
    )
    commands.add_parser("adopt", help="set up a worktree that already exists")
    commands.add_parser("env", help="print the exports this worktree needs")
    commands.add_parser("list", help="every checkout and the resources it owns")
    gone = commands.add_parser("remove", help="remove a worktree and its data")
    gone.add_argument("suffix")
    gone.add_argument("--keep-data", action="store_true", help="leave its QUICK SIGN IN accounts folder")
    arguments = parser.parse_args()

    if arguments.command == "create":
        return create(arguments.suffix, arguments.branch, arguments.own_runtime)
    if arguments.command == "adopt":
        return adopt()
    if arguments.command == "env":
        return environment()
    if arguments.command == "list":
        return show()
    return remove(arguments.suffix, arguments.keep_data)


if __name__ == "__main__":
    sys.exit(main())
