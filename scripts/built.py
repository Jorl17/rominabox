"""The path of this checkout's own `rominabox-cli`, freshly built.

The shared target causes two problems, and both give plausible wrong answers
instead of errors.

Scripts cannot rely on `desktop/src-tauri/target/release/rominabox-cli`.
The build output goes there only when `CARGO_TARGET_DIR` is unset, and we
set it in every worktree: in `worktree.py env` we point all of them at one
shared target, because a target per checkout costs several gigabytes to
avoid a lock that only serialises actual compilation. In a worktree that
path contains a stale binary or nothing at all.

The shared target also makes `release/rominabox-cli` a single file, from
the last build in any checkout. A script in one checkout could then
photograph a menu, stage a design or measure an export with another
checkout's code.

So we read from the binary the checkout it was built in, and keep a copy
inside this checkout, outside the shared target. We build only when we must,
because taking the shared cargo lock every time a script runs would make us
wait in every script for whatever else is compiling.

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
    asked = subprocess.run(
        [str(binary), "where"], capture_output=True, text=True, timeout=30
    )
    return asked.stdout.strip() if asked.returncode == 0 else None


def cli(build: bool = False) -> Path:
    """This checkout's command-line tool, checked as its own.

    We do not build it by default. Building would take the cargo lock every
    time any script runs, even for `--help`, and other checkouts share that
    lock, so we would wait in the script for a compile in another checkout.
    In the scripts we expect a binary that is already built.
    """
    mine = str((ROOT / "desktop/src-tauri").resolve())

    # The copy inside this checkout, if it exists and still comes from here.
    if owner(MINE) == mine and not build:
        return MINE

    # Otherwise the last build output, in the cargo target for this shell.
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
                f"no rominabox-cli belonging to this checkout, and it would not "
                f"build:\n{made.stderr.strip()[-800:]}"
            )

    for candidate in candidates:
        if owner(candidate) == mine:
            # We copy it inside this checkout, so that building in another
            # checkout cannot replace it before we use it. All worktrees share
            # one cargo target directory, so release/rominabox-cli is a single
            # file that a build in any checkout may overwrite.
            MINE.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(candidate, MINE)
            return MINE

    said = {str(c): owner(c) for c in candidates}
    raise SystemExit(
        "no rominabox-cli here belongs to this checkout.\n"
        + "".join(f"  {where}\n    says {who or '(an older build that cannot say)'}\n"
                  for where, who in said.items())
        + f"  this checkout is\n    {mine}\n\n"
        "Every worktree shares one cargo target, so that binary is whichever\n"
        "checkout built last. Build it here:\n"
        "  cargo build --release --manifest-path desktop/src-tauri/Cargo.toml "
        "--bin rominabox-cli"
    )


if __name__ == "__main__":
    print(cli(build="--build" in sys.argv))
