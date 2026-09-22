"""The path of this checkout's own `rominabox-cli`.

The shared target causes two problems, and both give plausible wrong answers
instead of errors.

Scripts cannot rely on `desktop/src-tauri/target/release/rominabox-cli`. The
build output goes there only when `CARGO_TARGET_DIR` is unset, and we set it
in every worktree: in `worktree.py env` we point all of them at one shared
target, because a target per checkout costs several gigabytes to avoid a
lock that only serialises actual compilation. In a worktree that path
contains a stale binary or nothing at all.

The shared target also makes `release/rominabox-cli` a single file, from the
last build in any checkout. A script in one checkout could then photograph a
menu, stage a design or measure an export with another checkout's code.

    from built import cli
    subprocess.run([str(cli()), "stage-theme"], ...)
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "desktop/src-tauri/Cargo.toml"
MINE = ROOT / "work/bin/rominabox-cli"


def target_dir() -> Path:
    """The cargo target folder, which is not always beside the manifest."""
    shared = os.environ.get("CARGO_TARGET_DIR")
    return Path(shared) if shared else ROOT / "desktop/src-tauri/target"


def owner(binary: Path) -> str | None:
    """The checkout this binary was built in, or None if we cannot read it."""
    if not binary.is_file():
        return None
    try:
        asked = subprocess.run(
            [str(binary), "where"], capture_output=True, text=True, timeout=30
        )
    except (subprocess.TimeoutExpired, OSError):
        return None
    return asked.stdout.strip() if asked.returncode == 0 else None


def cli(build: bool = False) -> Path:
    """This checkout's command-line tool, checked as its own.

    We do not build it by default. Building would take the cargo lock every
    time any script runs, even for `--help`, and other checkouts share that
    lock, so we would wait in the script for a compile in another checkout.
    In the scripts we expect a binary that is already built.

    This check does not show that the binary is current with the source,
    only which checkout it comes from. `where` prints a path compiled into
    the binary, not a hash of the code, so a binary built here last week
    still contains the path of this checkout. Pass `build=True` when it must
    be current.
    """
    mine = str((ROOT / "desktop/src-tauri").resolve())

    if not build and owner(MINE) == mine:
        return MINE

    candidates = [target_dir() / "release/rominabox-cli"]
    local = ROOT / "desktop/src-tauri/target/release/rominabox-cli"
    if local not in candidates:
        candidates.append(local)

    if build or not any(owner(c) == mine for c in candidates):
        made = subprocess.run(
            ["cargo", "build", "--quiet", "--release",
             "--manifest-path", str(MANIFEST), "--bin", "rominabox-cli"],
            capture_output=True, text=True,
        )
        if made.returncode != 0:
            raise SystemExit(
                "no rominabox-cli belonging to this checkout, and it would "
                f"not build:\n{made.stderr.strip()[-800:]}"
            )

    for candidate in candidates:
        if owner(candidate) != mine:
            continue
        # We keep it inside this checkout, so that building in another one
        # cannot replace it later. With the shared cargo target,
        # release/rominabox-cli is one file for every checkout.
        MINE.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(candidate, MINE)
        # We check the copy again. Checking the shared file and copying it are
        # two steps, and someone building in another checkout can replace that
        # file by a rename between them, so we could return a copy unchecked.
        if owner(MINE) != mine:
            raise SystemExit(
                f"{candidate} changed while it was being copied — another "
                "checkout built during the copy. Run this again."
            )
        return MINE

    said = {str(c): owner(c) for c in candidates}
    raise SystemExit(
        "no rominabox-cli here belongs to this checkout.\n"
        + "".join(
            f"  {where}\n    says {who or '(an older build that cannot say)'}\n"
            for where, who in said.items()
        )
        + f"  this checkout is\n    {mine}\n\n"
        "Every worktree shares one cargo target, so that binary is whichever\n"
        "checkout built last. Build it here:\n"
        "  cargo build --release --manifest-path desktop/src-tauri/Cargo.toml "
        "--bin rominabox-cli"
    )


if __name__ == "__main__":
    print(cli(build="--build" in sys.argv))
