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

So we build it, read from the binary the checkout it was built in, and keep
a copy inside this checkout, outside the shared target.

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


def cli(rebuild: bool = True) -> Path:
    """Return this checkout's command-line tool, built and checked as its own."""
    if rebuild:
        built = subprocess.run(
            ["cargo", "build", "--quiet", "--release",
             "--manifest-path", str(MANIFEST), "--bin", "rominabox-cli"],
            capture_output=True, text=True,
        )
        if built.returncode != 0:
            raise SystemExit(
                f"could not build rominabox-cli:\n{built.stderr.strip()[-800:]}"
            )

    fresh = target_dir() / "release/rominabox-cli"
    if not fresh.is_file():
        raise SystemExit(f"cargo reported success but {fresh} is not there")

    where = subprocess.run([str(fresh), "where"], capture_output=True, text=True)
    belongs = where.stdout.strip()
    expected = str((ROOT / "desktop/src-tauri").resolve())
    if where.returncode != 0 or belongs != expected:
        raise SystemExit(
            f"the built rominabox-cli belongs to another checkout.\n"
            f"  it says     {belongs or '(nothing — an older build with no `where`)'}\n"
            f"  this is     {expected}\n\n"
            f"Every worktree shares one cargo target, so that binary is whichever\n"
            f"checkout built last. Build again here and nothing else at the same\n"
            f"time, or give this checkout its own target directory."
        )

    # We keep it inside this checkout, so that building in another checkout
    # cannot replace the binary before we use it.
    MINE.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(fresh, MINE)
    return MINE


if __name__ == "__main__":
    print(cli(rebuild="--no-build" not in sys.argv))
