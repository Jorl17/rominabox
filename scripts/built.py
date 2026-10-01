"""The path of this checkout's own `rominabox-cli`: the release build in this
checkout's cargo target, which we build again when its sources are newer.

    from built import cli
    subprocess.run([str(cli()), "stage-theme"], ...)
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# The builder's Rust workspace (desktop/Cargo.toml), relative to a checkout.
# The engine and command line are in a package beside the builder's window
# (desktop/src-tauri), so we rebuild only the window when a bundled resource changes.
WORKSPACE = Path("desktop")
ENGINE = WORKSPACE / "crates/rominabox-engine"
MANIFEST = ROOT / ENGINE / "Cargo.toml"
# Executables have the .exe suffix on Windows and no suffix on POSIX systems.
NAME = "rominabox-cli.exe" if os.name == "nt" else "rominabox-cli"


def cli_build(manifest: Path = MANIFEST) -> list[str]:
    """The command to build the command line from `manifest`, with the features
    of its dependencies resolved over the whole workspace and the feature we
    pass in the Tauri build. The engine then has the same features for the
    command line as for the builder, and we compile it once instead of twice."""
    return ["cargo", "build", "--release", "--manifest-path", str(manifest), "--workspace",
            "--features", "tauri/custom-protocol", "--bin", "rominabox-cli"]


def target_dir() -> Path:
    """The cargo target folder, which is not always beside the manifest."""
    shared = os.environ.get("CARGO_TARGET_DIR")
    return Path(shared) if shared else ROOT / WORKSPACE / "target"


# The sources of the command line: the crates, its own among them, and the
# manifests of the workspace, which declare its dependencies and features.
SOURCES = [
    WORKSPACE / "crates",
    WORKSPACE / "Cargo.toml",
    WORKSPACE / "src-tauri/Cargo.toml",
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
    """This checkout's command-line tool.

    We do not build it by default. Building would take the cargo lock every
    time any script runs, even for `--help`, so we would wait in a script for
    a compile we do not require. In the scripts we expect a binary that is
    already built.

    We rebuild it when the source is newer than the binary. Without that check
    we would return a binary built before a merge, and in the menu state tests
    we would get a wrong answer, not an error, for a design that is not in
    that binary.
    """
    binary = target_dir() / "release" / NAME
    stale = not binary.is_file() or binary.stat().st_mtime < newest_source()
    if build or stale:
        made = subprocess.run(cli_build(), capture_output=True, text=True)
        if made.returncode != 0:
            raise SystemExit(f"rominabox-cli would not build:\n{made.stderr.strip()[-800:]}")
    if not binary.is_file():
        raise SystemExit(f"cargo built no rominabox-cli at {binary}")
    return binary


if __name__ == "__main__":
    print(cli(build="--build" in sys.argv))
