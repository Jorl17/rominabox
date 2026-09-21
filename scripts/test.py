"""Run one scope of the test suite, or all of them.

The tests are in five places, with five test runners, and while working you
rarely need all of them. After a change to a controller declaration you can
skip the frontend tests, and after a React change the ten overlay renders.

    python3 scripts/test.py                 # the fast scopes
    python3 scripts/test.py catalog         # one scope
    python3 scripts/test.py catalog menu    # several
    python3 scripts/test.py --all           # everything, including slow
    python3 scripts/test.py --list          # what exists and what it covers

Run with --list to see what each scope tests and what it leaves out. None of
the scopes shows that a game runs, which requires the player.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRATCH = ROOT / "work/test-output"

CARGO_DESKTOP = ["--manifest-path", str(ROOT / "desktop/src-tauri/Cargo.toml")]
CARGO_CATALOG = ["--manifest-path", str(ROOT / "desktop/crates/rominabox-catalog/Cargo.toml")]


class Scope:
    def __init__(self, name: str, covers: str, not_covered: str, command: list[str], slow: bool = False):
        self.name = name
        self.covers = covers
        self.not_covered = not_covered
        self.command = command
        self.slow = slow


SCOPES = [
    Scope(
        "catalog",
        "console packages parse, validate and generate the shipped registries",
        "that the generated data is correct for any real console, only that it is self-consistent",
        ["cargo", "test", "--quiet", *CARGO_CATALOG],
    ),
    Scope(
        "exporter",
        "the Rust exporter and player-facing declarations: staging, isolation, controls, capabilities",
        "that an exported game runs; every fixture core is an empty file",
        ["cargo", "test", "--quiet", *CARGO_DESKTOP],
    ),
    Scope(
        "frontend",
        "the React builder UI: controls editor, sound preview, app flow",
        "anything about the exported player, which is a different codebase",
        ["npm", "--prefix", str(ROOT / "desktop"), "test"],
    ),
    Scope(
        "menu",
        "what RmlUi does with the real menu.rml when clicked: hit testing, hover, focus, classes",
        "that the menu looks right, or anything about the C++ bridge, which is not loaded",
        ["python3", str(ROOT / "scripts/menu_interaction.py"), "--check"],
    ),
    Scope(
        "staging",
        "that the runtime-kit staging script names paths that exist, after any rename",
        "that the script runs or produces a correct kit; it builds a whole application",
        ["python3", str(ROOT / "scripts/test_staging.py")],
    ),
    Scope(
        "worktree",
        "isolation between parallel checkouts: the shared git dir, the lock, refusing the canonical tree",
        "that a real worktree builds or runs; it creates nothing outside a temporary directory",
        ["python3", str(ROOT / "scripts/test_worktree.py")],
    ),
    Scope(
        "bridge",
        "the RmlUi bridge itself, compiled with a dummy renderer: actions, hover, focus, capture",
        "anything in rmlui.c, which is not linked here, so control binding and keyboard order are untested",
        ["bash", str(ROOT / "scripts/native_runtime/test_rmlui_interaction.sh")],
        slow=True,
    ),
    Scope(
        "artwork",
        "that every controller PNG still matches a fresh render of its SVG source",
        "that the artwork is correct — only that the PNG has not diverged from the drawing",
        ["python3", str(ROOT / "scripts/render_controllers.py"), "--check"],
        slow=True,
    ),
    Scope(
        "overlays",
        "that no controller callout or button anchor moved, across every illustrated profile",
        "that the positions are correct — only that they are unchanged since a human looked",
        ["python3", str(ROOT / "scripts/render_control_overlays.py"), "--check", str(SCRATCH / "overlays")],
        slow=True,
    ),
]

BY_NAME = {scope.name: scope for scope in SCOPES}


def run(scope: Scope) -> tuple[bool, float]:
    started = time.monotonic()
    result = subprocess.run(scope.command, cwd=ROOT)
    return result.returncode == 0, time.monotonic() - started


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("scopes", nargs="*", help="which scopes to run; default is the fast ones")
    parser.add_argument("--all", action="store_true", help="every scope, including slow ones")
    parser.add_argument("--list", action="store_true", help="describe the scopes and exit")
    arguments = parser.parse_args()

    if arguments.list:
        for scope in SCOPES:
            mark = " (slow)" if scope.slow else ""
            print(f"{scope.name}{mark}\n  covers    {scope.covers}\n  does not  {scope.not_covered}\n")
        return 0

    if arguments.scopes:
        unknown = [name for name in arguments.scopes if name not in BY_NAME]
        if unknown:
            raise SystemExit(f"unknown scope(s): {', '.join(unknown)}; try --list")
        selected = [BY_NAME[name] for name in arguments.scopes]
    elif arguments.all:
        selected = SCOPES
    else:
        selected = [scope for scope in SCOPES if not scope.slow]

    SCRATCH.mkdir(parents=True, exist_ok=True)
    results = []
    for scope in selected:
        print(f"\n=== {scope.name} ===", flush=True)
        passed, seconds = run(scope)
        results.append((scope.name, passed, seconds))

    print("\n" + "=" * 46)
    for name, passed, seconds in results:
        print(f"{'PASS' if passed else 'FAIL'}  {name:<12}{seconds:6.1f}s")
    failed = [name for name, passed, _ in results if not passed]
    if failed:
        print(f"\n{len(failed)} scope(s) failed: {', '.join(failed)}")
        return 1
    if not arguments.all and not arguments.scopes:
        print("\nSlow scopes were skipped. Run --all before a checkpoint.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
