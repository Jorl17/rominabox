"""Create, adopt and remove isolated worktrees.

Two checkouts share more than is apparent: a dev-server port pinned with
`strictPort`, two macOS bundle identifiers, a per-game data directory under
Application Support, and a cargo target. None of those collisions produces
an error. Each one produces a plausible wrong answer, the worst being a
screenshot of a build from the other checkout.

    python3 scripts/worktree.py create feature-x     # make one
    python3 scripts/worktree.py adopt               # from inside an existing one
    python3 scripts/worktree.py list                # what exists, and its resources
    python3 scripts/worktree.py remove feature-x    # and clean up after it

Isolation is opt-in. With no `worktree.local.json` in a checkout, nothing
changes: the port is 1420, the identifiers have no suffix, and
`stable_identity` is the same. This is important, because a regression test
checks it so that a player's saves survive a re-export of the same game.

We do NOT isolate the prepared runtime kit, the cargo target directory and
the submodule's object store. An isolated 6 GB target would cost 6 GB per
worktree to avoid a lock that only serialises compilation, and isolated
submodule objects would mean a new clone of 289 MB that git can share at
no cost. We share large artifacts that depend only on their content.
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
DATA_HOME = Path.home() / "Library/Application Support"
CANONICAL_DATA = DATA_HOME / "ROM-in-a-Box"

# We allocate ports one offset at a time and store the offset in git's
# per-worktree metadata, never in a committed file, because it is local state
# and `git worktree remove` deletes it with everything else.
OFFSET_FILE = "rominabox_port_offset"
LOCK_NAME = "rominabox-worktree-lock"
MAX_OFFSET = 200


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
    allocation would not serialise, and a separate cargo target, so nothing
    would be shared.
    """
    reported = Path(git("rev-parse", "--git-common-dir"))
    return reported if reported.is_absolute() else (ROOT / reported).resolve()


def worktrees() -> list[dict]:
    """Return every checkout known to git, with its suffix and resources."""
    entries: list[dict] = []
    current: dict = {}
    for line in git("worktree", "list", "--porcelain").splitlines():
        if not line:
            if current:
                entries.append(current)
            current = {}
            continue
        key, _, value = line.partition(" ")
        if key == "worktree":
            current = {"path": Path(value)}
        elif key == "branch":
            current["branch"] = value.rsplit("/", 1)[-1]
        elif key == "detached":
            current["branch"] = "(detached)"
    if current:
        entries.append(current)
    for entry in entries:
        config = entry["path"] / LOCAL_CONFIG
        entry["local"] = json.loads(config.read_text()) if config.exists() else None
    return entries


def port_is_free(port: int) -> bool:
    """Return whether anything listens on the port, not only whether we reserved it.

    We keep the reservation of a stopped sibling worktree, but an orphaned
    process or an unrelated application can use a port without any
    reservation, and we can learn about those only from the system.
    """
    probe = subprocess.run(
        ["lsof", "-i", f":{port}", "-sTCP:LISTEN"],
        capture_output=True,
        text=True,
    )
    return probe.returncode != 0 or not probe.stdout.strip()


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
        try:
            os.kill(holder, 0)
            return False
        except ProcessLookupError:
            pass
        except PermissionError:
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
        "gameBundlePrefix": f"app.rominabox.game.wt-{suffix}",
        "dataRoot": str(DATA_HOME / f"ROM-in-a-Box-wt-{suffix}"),
    }
    (path / LOCAL_CONFIG).write_text(json.dumps(local, indent=2) + "\n")
    return local


# The prepared runtime kit (the frozen RetroArch player, the cores, the menu
# assets) is build output and is not in git. Without it we cannot export or
# launch anything in a worktree, and a RetroArch build per worktree is too slow.
# It is the same in every worktree unless the fork or a design changes, so we
# share it with a symlink.
#
# Staging a design through that symlink would write into the canonical
# checkout and give every other worktree a kit that does not match its
# design. So in `build-builder-macos.sh` we refuse to stage through a shared
# kit and suggest `--own-runtime`.
SHARED_ARTIFACTS = [
    Path("desktop/src-tauri/resources/runtime"),
    Path("desktop/src-tauri/resources/preview"),
    # Without these we cannot run the whole suite in a worktree, and the
    # failures look like a fault in the change. The `frontend` tests stop with
    # "tsc: command not found", and in `menu` and `bridge` we cannot link RmlUi.
    # Both are build output, the same in every worktree, and not in git.
    Path("desktop/node_modules"),
    Path("work/experiments"),
    # These are bundled resources in the tauri build, so without them we cannot
    # compile the desktop crate. The error, "resource path `resources/skills`
    # doesn't exist", looks like a missing file and not a missing link, and
    # most test scopes then fail.
    Path("desktop/src-tauri/resources/bin"),
    Path("desktop/src-tauri/resources/skills"),
    # The catalogues and picture lists for the identification measurement. We
    # fetch them on purpose and never during a test, so without them we cannot
    # run those tests in a worktree. The error message suggests a fetch, and in
    # every checkout that would mean many requests to another party's API for
    # the same files.
    Path("work/identification-cache"),
]


def link_build_artifacts(path: Path, own_copy: bool) -> None:
    for relative in SHARED_ARTIFACTS:
        source = ROOT / relative
        if not source.exists():
            print(f"  {relative} is not prepared here; skipping")
            continue
        target = path / relative
        if target.exists() or target.is_symlink():
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        if own_copy:
            started = time.monotonic()
            shutil.copytree(source, target, symlinks=True)
            print(f"  copied {relative} ({time.monotonic() - started:.0f}s)")
        else:
            target.symlink_to(source)
            print(f"  linked {relative} -> the canonical checkout")

def describe(local: dict) -> str:
    return (
        f"  port        {local['vitePort']}\n"
        f"  builder id  {local['builderBundleId']}\n"
        f"  game ids    {local['gameBundlePrefix']}.*\n"
        f"  data root   {local['dataRoot']}"
    )


def create(suffix: str, branch: str | None, own_runtime: bool) -> int:
    path = ROOT.parent / f"{ROOT.name}-{suffix}"
    if path.exists():
        raise SystemExit(f"{path} already exists; use adopt, or pick another suffix")
    arguments = ["worktree", "add"]
    if branch:
        arguments += ["-b", branch]
    arguments.append(str(path))
    print(git(*arguments))

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


def adopt() -> int:
    """Set up a worktree created by something else, such as a bare git command."""
    here = Path.cwd().resolve()
    known = {entry["path"].resolve(): entry for entry in worktrees()}
    if here not in known:
        raise SystemExit(f"{here} is not a git worktree of this repository")
    if here == ROOT:
        raise SystemExit("this is the canonical checkout; it is never suffixed")
    suffix = here.name.removeprefix(f"{ROOT.name}-") or here.name
    existing = known[here].get("local") or {}
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
    shared_target = common_dir() / "shared-cargo-target"
    print(f"export ROMINABOX_VITE_PORT={local['vitePort']}")
    print(f"export ROMINABOX_BUNDLE_ID={local['builderBundleId']}")
    print(f"export ROMINABOX_GAME_BUNDLE_PREFIX={local['gameBundlePrefix']}")
    print(f"export ROMINABOX_DATA_ROOT={json.dumps(local['dataRoot'])}")
    # We share this on purpose. See the module docstring.
    print(f"export CARGO_TARGET_DIR={json.dumps(str(shared_target))}")
    return 0


def show() -> int:
    for entry in worktrees():
        local = entry.get("local")
        marker = "canonical" if entry["path"].resolve() == ROOT else "worktree"
        print(f"{entry['path']}  [{entry.get('branch', '?')}]  {marker}")
        print(describe(local) if local else "  (not set up — run adopt inside it)")
    return 0


def remove(suffix: str, keep_data: bool) -> int:
    path = ROOT.parent / f"{ROOT.name}-{suffix}"
    entry = next((e for e in worktrees() if e["path"].resolve() == path.resolve()), None)
    if entry is None:
        raise SystemExit(f"no worktree at {path}")
    local = entry.get("local")

    # The data directory is outside the worktree, so `git worktree remove`
    # does not delete it, and we delete it by hand in teardown.
    if local and not keep_data:
        data_root = Path(local["dataRoot"])
        if data_root.exists() and data_root != CANONICAL_DATA:
            shutil.rmtree(data_root)
            print(f"removed {data_root}")
    # A shared kit is a symlink into the canonical checkout. When we remove the
    # worktree we must unlink it and never follow it.
    for relative in SHARED_ARTIFACTS:
        link = path / relative
        if link.is_symlink():
            link.unlink()
    git("worktree", "remove", "--force", str(path))
    print(f"removed {path}")
    print("shared runtime kit, cargo target and submodule objects were left alone")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    made = commands.add_parser("create", help="make an isolated worktree")
    made.add_argument("suffix")
    made.add_argument("--branch", help="branch to create; default is git's own naming")
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
    gone.add_argument("--keep-data", action="store_true", help="leave its Application Support directory")
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
