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
import json
import os
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cargo_replay import cargo_test  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
SCRATCH = ROOT / "work/test-output"
BUDGETS = ROOT / "scripts/fixtures/scope-budgets.json"
PRINT_LOCK = threading.Lock()

CARGO_DESKTOP = ["--manifest-path", str(ROOT / "desktop/src-tauri/Cargo.toml")]
CARGO_CATALOG = ["--manifest-path", str(ROOT / "desktop/crates/rominabox-catalog/Cargo.toml")]


class Scope:
    def __init__(
        self,
        name: str,
        covers: str,
        not_covered: str,
        command: list[str],
        slow: bool = False,
        prepare: list[list[str]] | None = None,
    ):
        self.name = name
        self.covers = covers
        self.not_covered = not_covered
        self.command = command
        self.slow = slow
        # What we must stage before the scope runs. We declare it here because
        # without the declaration, a scope with generated input would work only
        # in a checkout where someone had generated it, and fail everywhere else.
        self.prepare = prepare or []


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
        "picture",
        "that a hard edge in a core's picture is still a hard edge after the options an export ships",
        "window placement, bilinear scaling of an already-sharp frame, or any console whose core is not in the kit",
        ["python3", str(ROOT / "scripts/picture_edges.py")],
    ),
    Scope(
        "frontend",
        "the React builder UI: controls editor, sound preview, app flow, and that it typechecks",
        "anything about the exported player, which is a different codebase",
        # With `npm test` we run vitest, without a type check, so we include the
        # type check in this scope instead of leaving it to a full build.
        # With `npm run check` we also stage the geometry, so in this scope and
        # in `npm run build` (the same type check, in the tauri build) we use
        # the same staged layout JSON on a clean checkout.
        ["npm", "--prefix", str(ROOT / "desktop"), "run", "check"],
    ),
    Scope(
        "menu",
        "what RmlUi does with the real menu.rml when clicked: hit testing, hover, focus, classes",
        "that the menu looks right, or anything about the C++ bridge, which is not loaded",
        ["python3", str(ROOT / "scripts/menu_interaction.py"), "--check"],
    ),
    Scope(
        "heldkey",
        "that Escape still toggles the menu while another key is held, including a press that starts and ends between two samples",
        "that a physical keyboard delivers the events; the decision is the function the runloop calls",
        ["bash", str(ROOT / "scripts/test_held_key.sh")],
    ),
    Scope(
        "staging",
        "that the runtime-kit staging script names paths that exist, after any rename",
        "that the script runs or produces a correct kit; it builds a whole application",
        ["python3", str(ROOT / "scripts/test_staging.py")],
    ),
    Scope(
        "joypad",
        "that every hid profile the pin declares is staged, and that RetroArch's match rules would accept it",
        "that a physical pad's buttons match those numbers; nothing here opens a device",
        ["python3", str(ROOT / "scripts/test_joypad_autoconfig.py")],
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
        "states",
        "that every declared menu state still renders in every palette from desktop/designs.json, with its artwork, and looks the same",
        "that the bridge sets those classes at the right moment; the bridge scope covers that",
        ["python3", str(ROOT / "scripts/menu_states.py"), "--check"],
        slow=True,
    ),
    Scope(
        "placement",
        "that the controller picker lands in the same place on every console that offers one",
        "that the place is a good one — only that it is the same one, whichever pad is drawn",
        ["python3", str(ROOT / "scripts/menu_states.py"), "--fixed-place", str(SCRATCH / "picker-place")],
        slow=True,
    ),
    Scope(
        "variants",
        "that every controller a player can pick has artwork staged and a scene to swap to",
        "that the player actually swaps to it; that is the bridge, and rmlui.c is not linked here",
        ["python3", str(ROOT / "scripts/menu_states.py"), "--every-variant", str(SCRATCH / "variants")],
        slow=True,
    ),
    Scope(
        "identification",
        "how many catalogue names get the correct cover, against every published picture list",
        "that every cover downloads, or that a real ROM was hashed; it matches names to the published filenames and checks one pointer file",
        [
            "cargo",
            "test",
            "--manifest-path",
            str(ROOT / "desktop/src-tauri/Cargo.toml"),
            "--lib",
            "measure::",
            "--",
            "--ignored",
            "--nocapture",
        ],
        slow=True,
    ),
    Scope(
        "automation",
        "that something other than a person still runs this suite",
        "that the hook is installed in a fresh clone; core.hooksPath is local configuration",
        ["python3", str(ROOT / "scripts/test_automation.py")],
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


def execute(command: list[str]) -> subprocess.CompletedProcess:
    if command and command[0] == "cargo" and "test" in command[:2]:
        return cargo_test(command, ROOT)
    return subprocess.run(
        command, cwd=ROOT, capture_output=True, text=True, errors="replace"
    )


def run(scope: Scope) -> tuple[bool, float, str]:
    started = time.monotonic()
    chunks: list[str] = []
    for step in scope.prepare:
        staged = execute(step)
        chunks.append(staged.stdout or "")
        chunks.append(staged.stderr or "")
        if staged.returncode != 0:
            chunks.append(f"  could not stage what {scope.name} needs\n")
            return False, time.monotonic() - started, "".join(chunks)
    result = execute(scope.command)
    chunks.append(result.stdout or "")
    chunks.append(result.stderr or "")
    return result.returncode == 0, time.monotonic() - started, "".join(chunks)


def load_budgets() -> dict | None:
    if not BUDGETS.is_file():
        return None
    return json.loads(BUDGETS.read_text())


def over_budget(seconds: float, budget: float, limits: dict) -> bool:
    """Return twice the time, and at least `slack` seconds more, so that on a
    busy machine we do not fail a scope that took a moment longer."""
    slack = float(limits.get("slack_seconds", 10))
    return seconds > max(budget * 2, budget + slack)


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
    recorded: dict[str, tuple[bool, float]] = {}
    wall_started = time.monotonic()

    def finish(scope: Scope) -> None:
        with PRINT_LOCK:
            print(f"start {scope.name}", flush=True)
        passed, seconds, output = run(scope)
        with PRINT_LOCK:
            print(f"\n=== {scope.name} ===", flush=True)
            if output:
                print(output, end="" if output.endswith("\n") else "\n", flush=True)
            recorded[scope.name] = (passed, seconds)

    # Four at a time. With more, the renders and the test binaries wait for
    # each other and the run is no faster. We run Cargo one at a time in
    # cargo_replay, because the target directory is shared.
    workers = min(4, len(selected), os.cpu_count() or 4)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        pending = [pool.submit(finish, scope) for scope in selected]
        for future in as_completed(pending):
            future.result()

    wall = time.monotonic() - wall_started
    limits = load_budgets()
    scope_budgets = (limits or {}).get("scopes") or {}
    print("\n" + "=" * 46)
    slow: list[str] = []
    for scope in selected:
        passed, seconds = recorded[scope.name]
        budget = scope_budgets.get(scope.name)
        ratio = f"  {seconds / budget:4.1f}x" if budget else ""
        mark = "PASS" if passed else "FAIL"
        if passed and budget is not None and limits is not None and over_budget(seconds, budget, limits):
            mark = "SLOW"
            slow.append(scope.name)
        print(f"{mark}  {scope.name:<12}{seconds:6.1f}s{ratio}")
    wall_budget = (limits or {}).get("wall")
    wall_ratio = f"  {wall / wall_budget:4.1f}x" if wall_budget else ""
    print(f"\nwall {wall:0.1f}s{wall_ratio}")
    if wall_budget and limits is not None and over_budget(wall, wall_budget, limits):
        slow.append("wall")
    failed = [scope.name for scope in selected if not recorded[scope.name][0]]
    if failed:
        print(f"\n{len(failed)} scope(s) failed: {', '.join(failed)}")
        return 1
    if slow:
        print(
            f"\n{len(slow)} timing(s) exceeded the budget in {BUDGETS.name}: {', '.join(slow)}\n"
            "A budget is the last measured time for that scope. It is exceeded "
            "when a run takes more than twice as long and at least the slack longer."
        )
        return 1
    if not arguments.all and not arguments.scopes:
        print("\nSlow scopes were skipped. Run --all before a checkpoint.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
