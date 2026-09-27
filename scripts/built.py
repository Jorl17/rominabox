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
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "desktop/src-tauri/Cargo.toml"
# Executables have the .exe suffix on Windows and no suffix on POSIX systems.
NAME = "rominabox-cli.exe" if os.name == "nt" else "rominabox-cli"
MINE = ROOT / "work/bin" / NAME


def target_dir() -> Path:
    """The cargo target folder, which is not always beside the manifest."""
    shared = os.environ.get("CARGO_TARGET_DIR")
    return Path(shared) if shared else ROOT / "desktop/src-tauri/target"


def owner(binary: Path) -> str | None:
    """The checkout this binary was built in, or None if we cannot read it."""
    if not binary.is_file():
        return None
    try:
        # We send `where` no request. We close stdin, so a pipe left open by
        # whoever launched this cannot make us wait for the answer.
        asked = subprocess.run(
            [str(binary), "where"],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (subprocess.TimeoutExpired, OSError):
        return None
    return asked.stdout.strip() if asked.returncode == 0 else None


SOURCES = [
    Path("desktop/src-tauri/src"),
    Path("desktop/src-tauri/Cargo.toml"),
    Path("desktop/crates"),
]

# `include_str!("../../controls.json")` and similar macros: files that are not
# Rust but are compiled into the binary, so editing one makes the built tool
# stale exactly as editing a `.rs` file does. We read them out of the sources
# instead of listing them here, because a list would be a second copy of the
# same fact and could disagree with the sources.
_INCLUDES = re.compile(r"""include_(?:str|bytes)!\s*\(\s*"([^"]+)"\s*\)""")


def _rust_files() -> "list[Path]":
    found = []
    for relative in SOURCES:
        path = ROOT / relative
        if path.is_dir():
            found.extend(path.rglob("*.rs"))
    return found


def compiled_in(source: Path) -> "list[Path]":
    """The non-Rust files that we compile into the binary from this source."""
    try:
        text = source.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return []
    return [(source.parent / captured).resolve() for captured in _INCLUDES.findall(text)]


def newest_source() -> float:
    """The time of the last change to anything we build the exporter from."""
    newest = 0.0
    for relative in SOURCES:
        path = ROOT / relative
        if path.is_file():
            newest = max(newest, path.stat().st_mtime)
        elif path.is_dir():
            for found in path.rglob("*.rs"):
                newest = max(newest, found.stat().st_mtime)
            for found in path.rglob("Cargo.toml"):
                newest = max(newest, found.stat().st_mtime)
    for source in _rust_files():
        for baked in compiled_in(source):
            if baked.is_file():
                newest = max(newest, baked.stat().st_mtime)
    return newest


def cli(build: bool = False) -> Path:
    """This checkout's command-line tool, checked as its own.

    We do not build it by default. Building would take the cargo lock every
    time any script runs, even for `--help`, and other checkouts share that
    lock, so we would wait in the script for a compile in another checkout.
    In the scripts we expect a binary that is already built.

    We rebuild it when the source is newer than the binary. `where` prints a
    path compiled into a binary, from which we learn its checkout but not its
    age. Without the mtime check we would return a binary built before a
    merge, and in the menu state tests we would get a wrong answer, not an
    error, for a design that is not in that binary.
    """
    mine = str((ROOT / "desktop/src-tauri").resolve())

    stale = MINE.is_file() and MINE.stat().st_mtime < newest_source()
    if not build and not stale and owner(MINE) == mine:
        return MINE
    build = build or stale

    candidates = [target_dir() / "release" / NAME]
    local = ROOT / "desktop/src-tauri/target/release" / NAME
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
        # We write it beside the target and rename it into place, which is
        # atomic. We call this from several test runs in parallel, so with a
        # direct copy one run could execute the file while another is halfway
        # through writing it.
        #
        # We skip this when the file already has the same bytes, which is the
        # usual case and costs one comparison.
        if not (MINE.is_file() and MINE.read_bytes() == candidate.read_bytes()):
            spare = MINE.with_suffix(f".{os.getpid()}")
            shutil.copy2(candidate, spare)
            os.replace(spare, MINE)
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
