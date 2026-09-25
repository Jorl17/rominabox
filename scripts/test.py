"""Run one scope of the test suite, or all of them.

The tests are in five places, with five test runners, and while working you
rarely need all of them. After a change to a controller declaration you can
skip the frontend tests, and after a React change the ten overlay renders.

    python3 scripts/test.py                 # the fast scopes
    python3 scripts/test.py catalog         # one scope
    python3 scripts/test.py catalog menu    # several
    python3 scripts/test.py --all           # everything, including slow
    python3 scripts/test.py --list          # what exists and what it covers

Run with --list to see what each scope tests and what it leaves out. Passing
the fast scopes does not show that a game runs. In the isolation scope we run
a game for a few frames, and we test window placement and fullscreen by hand.
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
import core_source  # noqa: E402
from cargo_replay import cargo_test  # noqa: E402
from player_support import additions as support_additions  # noqa: E402
from player_support import modifications as support_modifications  # noqa: E402
from player_support import snapshot as support_snapshot  # noqa: E402
from temp_entries import additions as temp_additions  # noqa: E402
from temp_entries import directory as temp_directory  # noqa: E402
from temp_entries import snapshot as temp_snapshot  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
SCRATCH = ROOT / "work/test-output"
BUDGETS = ROOT / "scripts/fixtures/scope-budgets.json"
PRINT_LOCK = threading.Lock()

CARGO_DESKTOP = ["--manifest-path", str(ROOT / "desktop/src-tauri/Cargo.toml")]
CARGO_CATALOG = ["--manifest-path", str(ROOT / "desktop/crates/rominabox-catalog/Cargo.toml")]
# bridge, dcmenu and menu link one RmlUi. We declare it in each scope that
# uses it, or in a checkout where nobody has compiled it the scope fails.
RMLUI_PREPARE = [["python3", str(ROOT / "scripts/prepare_rmlui.py")]]

# For the launched workflow cases we select a committed player explicitly and
# use isolated storage, in a namespace apart from the other native scopes.
WORKFLOW_COMMAND = [
    "env", f"ROMINABOX_GAME_BUNDLE_PREFIX={os.environ.get('ROMINABOX_GAME_BUNDLE_PREFIX', '')}.workflows",
    "python3", str(ROOT / "scripts/menu_workflows.py"),
]


# What we link into a headless menu driver beside the cached menu objects,
# which is the fake RetroArch host, its keyboard and achievements stand-ins,
# and the libretro-common files for reading the menu's files. We build every
# driver the same way, so all of them run the same menu.
HEADLESS_DEFINES = ["--define", "HAVE_AUDIOMIXER"]
HEADLESS_SUPPORT = [
    "scripts/native_runtime/menu_host_fake.cpp",
    "scripts/native_runtime/text_test_host.cpp",
    "scripts/native_runtime/achievements_fake.cpp",
    *(f"vendor/retroarch/libretro-common/{name}" for name in (
        "file/config_file.c", "file/file_path.c", "file/file_path_io.c",
        "streams/file_stream.c", "string/stdstring.c", "vfs/vfs_implementation.c",
        "encodings/encoding_utf.c", "time/rtime.c", "compat/compat_strl.c")),
]


def headless_driver(output: Path, source: str) -> list[str]:
    """Return the harness helper's command to build one headless menu driver."""
    return ["python3", str(ROOT / "scripts/native_runtime/menu_harness.py"), "build",
            str(output), *HEADLESS_DEFINES,
            *(str(ROOT / path) for path in [source, *HEADLESS_SUPPORT])]


# The navigation driver, and the driver the workflow cases replay through.
NAVIGATION_DRIVER = ROOT / "work/navigation/menu_nav_driver"
WORKFLOW_DRIVER = ROOT / "work/workflows/menu_workflow_driver"


class Scope:
    def __init__(
        self,
        name: str,
        covers: str,
        not_covered: str,
        command: list[str],
        slow: bool = False,
        prepare: list[list[str]] | None = None,
        skipped: str | None = None,
    ):
        self.name = name
        self.covers = covers
        self.not_covered = not_covered
        self.command = command
        self.slow = slow
        # Why we leave a scope out of an ordinary run. We still run it when named.
        self.skipped = skipped
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
        "the Rust exporter and player-facing declarations: staging, isolation, controls, capabilities, and that a command with no request does not read stdin",
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
        "shipped",
        "that the core options, remaps, controller profiles and firmware an export ships replace what an earlier export or data location left in the game's data on every launch, that a setting the player changed afterwards stays, that a player setting chosen in the game's menu reaches the next launch whatever default a later export carries, and that the launcher modules build for Windows",
        "that RetroArch reads those files or that the core honours them (the picture scope covers the core), or that the Windows build runs",
        ["python3", str(ROOT / "scripts/test_shipped.py")],
    ),
    Scope(
        "accounts",
        "the shared RetroAchievements accounts store on real files: who is listed, when an account leaves, private modes, unsafe names, several games changing it at once, and that it builds for Windows",
        "that a game calls it, that the sandbox lets a game reach the folder (the isolation scope), or that the Windows build runs",
        ["python3", str(ROOT / "scripts/test_accounts_store.py")],
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
        "builder",
        "that the browser build of the builder can be walked, that a dropped file's companions are named on the details step, and that shader packaging is on the menu step",
        "the desktop shell: catalog artwork, a rendered menu preview, firmware wording, and creating the app",
        ["python3", str(ROOT / "scripts/builder_shots.py"), "--check"],
        slow=True,
    ),
    Scope(
        "menu",
        "what RmlUi does with the real menu.rml when clicked: hit testing, hover, focus, classes",
        "that the menu looks right, or anything about the C++ bridge, which is not loaded",
        ["python3", str(ROOT / "scripts/menu_interaction.py"), "--check"],
        prepare=RMLUI_PREPARE,
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
        "padbinds",
        "that a control lists every input a press can come from, including the pad an autoconfig profile bound",
        "that a physical pad is detected; it compiles the real function and hands it binds it makes up",
        ["node", str(ROOT / "scripts/native_runtime/test_pad_bindings.mjs")],
    ),
    Scope(
        "glslcore",
        "that a core OpenGL context is given a GLSL version it accepts, and that a missing shader path is not passed to path_basename",
        "that a real context compiles the stock shader; it compiles the decision from the fork and hands it versions",
        ["node", str(ROOT / "scripts/native_runtime/test_glsl_core.mjs")],
    ),
    Scope(
        "dcmenu",
        "that a core-profile context draws the menu and a legacy context still does, and that a log line reaches the file before the process exits",
        "that a Dreamcast disc boots, or where the menu sits; the pictures are a separate run",
        ["python3", str(ROOT / "scripts/test_dcmenu.py")],
        prepare=RMLUI_PREPARE,
    ),
    Scope(
        "joypad",
        "that every hid profile the pin declares is staged, and that RetroArch's match rules would accept it",
        "that a physical pad's buttons match those numbers; nothing here opens a device",
        ["python3", str(ROOT / "scripts/test_joypad_autoconfig.py")],
    ),
    Scope(
        "reporoot",
        "that nothing finds the repository by the path it was compiled in, that no test or script names one person's ROM directory, and that no script or test names the removed experiment tree",
        "that the rule is right, or that a binary really came from elsewhere; it reads how each place asks",
        ["python3", str(ROOT / "scripts/test_repo_root.py")],
    ),
    Scope(
        "fixtures",
        "that a test file this repository does not generate is fetched or skipped out loud, and that the generated cartridge is ready",
        "that a fetched disc boots; the quit scope launches one, and only when the file is actually there",
        ["python3", str(ROOT / "scripts/test_fetch_content.py")],
    ),
    Scope(
        "symlinks",
        "that git carries no symbolic link, which would point somewhere else on every other machine",
        "that a worktree has the links it needs, or that the ignore rules are right",
        ["python3", str(ROOT / "scripts/test_no_symlinks.py")],
    ),
    Scope(
        "worktree",
        "isolation between parallel checkouts: the shared git dir, the lock, refusing the canonical tree, and that create will not check out an existing branch",
        "that a real worktree builds or runs; it creates nothing outside a temporary directory",
        ["python3", str(ROOT / "scripts/test_worktree.py")],
    ),
    Scope(
        "shotsign",
        "that replacing the shot player keeps the sandbox the export signed, and that every shot shares one bundle namespace",
        "that a picture was taken; that is menu_shots, and this does not launch a game",
        ["python3", str(ROOT / "scripts/test_shot_sign.py")],
    ),
    Scope(
        "achievement-client",
        "managed account, evaluator, pending uploads and state restoration using the real rcheevos client",
        "RetroArch core-memory mapping or its actual runloop/HTTP/save-task adapters",
        ["sh", str(ROOT / "scripts/achievements_runtime_test.sh")],
    ),
    Scope(
        "achievement-native",
        "actual exported core/client authentication, autosave restoration, unlock, OFF and exclusion against a loopback service",
        "a real RetroAchievements account or physical keyboard/controller behavior",
        ["python3", str(ROOT / "scripts/achievements_native_workflow.py")],
        slow=True,
        skipped="opt-in native launch: requires worktree.py env and ROMINABOX_TEST_BUILD pointing to a test-only achievements build",
    ),
    Scope(
        "account-input",
        "real RmlUi account input, Unicode/composition, logical navigation, notification lifetime and both design/palette combinations",
        "physical dead keys, native IME candidate placement, controllers or live login",
        ["sh", str(ROOT / "scripts/achievements_input_test.sh")],
        # The checks of the harness build itself. With a stale object from the
        # cache, bridge would compile against code that no longer exists.
        prepare=RMLUI_PREPARE + [["python3", str(ROOT / "scripts/native_runtime/test_menu_harness.py")]],
    ),
    Scope(
        "bridge",
        "the real menu and RmlUi document with a dummy renderer: actions, shared pointer/key focus, binding capacity, capture lifecycle, save/load failures and persistence",
        "physical input capture or audible sound; a fake RetroArch host controls the failure/capture boundary, and rendering appearance needs direct screenshot review",
        ["bash", str(ROOT / "scripts/native_runtime/test_rmlui_interaction.sh")],
        # We build bridge through the same cache, so we check the cache there too.
        prepare=RMLUI_PREPARE + [["python3", str(ROOT / "scripts/native_runtime/test_menu_harness.py")]],
    ),
    Scope(
        "navigation",
        "arrow keys, pointer, focus and their sounds on every screen of every registered design and of the hypothetical layouts, driven through the real menu C++ on composed documents",
        "physical keyboards, pads or mice, audible sound, or how a highlight looks; a fake RetroArch host stands in for the player and nothing is drawn",
        ["env", f"ROMINABOX_NAVIGATION_DRIVER={NAVIGATION_DRIVER}",
         "cargo", "test", "--quiet", *CARGO_DESKTOP, "--test", "menu_navigation", "--", "--include-ignored"],
        prepare=RMLUI_PREPARE + [headless_driver(NAVIGATION_DRIVER, "scripts/native_runtime/menu_nav_driver.cpp")],
    ),
    Scope(
        "workflows",
        "every menu workflow case (keys, pointer, controls, capture, volume, shaders, saves, overlays, accounts) in both designs and every palette, replayed through the fork's own script driver and report on menus composed as an export composes them, compared checkpoint by checkpoint and file by file with what the launched player recorded",
        "anything drawn, audible cues, RetroArch's bind descriptions and remap files, or physical input; the fake host stands in for RetroArch, and the workflows-native scope launches a few cases for real",
        ["env", f"ROMINABOX_WORKFLOW_DRIVER={WORKFLOW_DRIVER}",
         "cargo", "test", "--quiet", *CARGO_DESKTOP, "--test", "menu_workflows", "--", "--include-ignored"],
        prepare=RMLUI_PREPARE + [headless_driver(WORKFLOW_DRIVER, "scripts/native_runtime/menu_workflow_driver.cpp")],
    ),
    Scope(
        "workflows-native",
        "the few workflow cases menu-workflows.json marks `launched`, in the exported player itself: one per design and a save and load through RetroArch's own state task, compared with the same baselines as the headless replay, and each picture",
        "audible cues, physical input or native focus/fullscreen; inspect the captured images directly too",
        WORKFLOW_COMMAND + ["--output", str(SCRATCH / "menu-workflows")],
        slow=True,
        skipped="opt-in native launch: requires worktree.py env and ROMINABOX_TEST_BUILD for the exact committed player",
    ),
    Scope(
        "edges",
        "that a photographed open list and a photographed focused control have all four outline edges painted",
        "where the list was placed, or that the boxes in the bridge agree; it only reads the picture",
        ["python3", str(ROOT / "scripts/check_menu_edges.py")],
    ),
    Scope(
        "pictures",
        "that a badge still downloading draws a moving placeholder and a failed one a mark, in every design, and draws every Disc screen into work/feedback-pictures",
        "that the player sets those classes or keeps redrawing: the rows are written the way its list writes them, and the second moment is RmlUi's 0.1 s step",
        ["python3", str(ROOT / "scripts/menu_pictures.py")],
    ),
    Scope(
        "states",
        "that every declared menu state still renders in every palette from desktop/designs.json, with its artwork, and looks the same",
        "that the bridge sets those classes at the right moment; the bridge scope covers that",
        ["python3", str(ROOT / "scripts/menu_states.py"), "--check"],
        slow=True,
    ),
    Scope(
        "fallback",
        "the controls screen a console with no controller drawing gets, in every state and palette",
        "that its layout is good — only that every control is there and that hover, focus and capture still differ",
        ["python3", str(ROOT / "scripts/menu_states.py"), "--system", "atari2600", "--check"],
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
        "that the player actually swaps to it; that is the native menu, which is not linked here",
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
        "menupreview",
        "that the builder can draw its own preview of every design it offers, in every palette",
        "what the preview looks like — the states scope asks that; this asks whether it draws at all",
        ["python3", str(ROOT / "scripts/test_menu_preview.py")],
        slow=True,
    ),
    Scope(
        "shaderpreview",
        "that every shader's preview is still what that shader does to a picture, rendered from its own GLSL",
        "that the filter is a good one — only that the picture of it is made by running it",
        ["python3", str(ROOT / "scripts/render_shader_previews.py"), "--check"],
        slow=True,
    ),
    Scope(
        "size",
        "that an exported app stays under the size ceiling, and does not carry the video encoders",
        "a cartridge's own size, or that the player was rebuilt; it measures the kit already on disk",
        ["python3", str(ROOT / "scripts/size_bundles.py")],
    ),
    Scope(
        "isolation",
        "that a signed export keeps the sandbox entitlement, cannot read or write the player's RetroArch profile or another game's container, and still loads a core, stays quiet, and sees a gamepad",
        "window placement, focus, fullscreen, or that Gatekeeper accepts an ad-hoc signature",
        [
            "cargo",
            "test",
            "--manifest-path",
            str(ROOT / "desktop/src-tauri/Cargo.toml"),
            "--test",
            "isolation",
            "--",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ],
        slow=True,
    ),
    Scope(
        "overlays",
        "that no controller callout or button anchor moved, across every illustrated profile",
        "that the positions are correct — only that they are unchanged since a human looked",
        ["python3", str(ROOT / "scripts/render_control_overlays.py"), "--check", str(SCRATCH / "overlays")],
        slow=True,
    ),
    Scope(
        "shaderstate",
        "that the shader row marked ON is the preset the running game is using, including after a restart",
        "that the filter looks right — only which row says it is the one on",
        ["python3", str(ROOT / "scripts/shader_state.py")],
        slow=True,
    ),
    Scope(
        "discs",
        "that a cartridge and a single disc are not a multi-disc game, that choosing the second image makes the core report that index, and that an exported game's menu does the same in both designs while a one-disc game hides the Disc entry and opens the circle",
        "that a disc name was shortened — the bridge measures that",
        ["python3", str(ROOT / "scripts/test_discs.py")],
        slow=True,
    ),
    Scope(
        "quit",
        "that an Apple Event quit of an exported Flycast game unloads the core before the process exits",
        "window placement and fullscreen; closing the window is the same AppKit terminate path",
        ["python3", str(ROOT / "scripts/test_quit.py")],
        slow=True,
        prepare=[["python3", str(ROOT / "scripts/fetch_test_content.py"), "--scope", "quit"]],
        skipped="its Dreamcast half needs a bootable Dreamcast image the repository does not have yet; name it to run the rest",
    ),
    Scope(
        "quiet",
        "quiet launch decisions, null audio, transparent windows, hands-on opt-outs, and safe native timeout handling",
        "actual GL presentation or hands-on focus/fullscreen; the window probe never orders its window in",
        ["python3", str(ROOT / "scripts/test_quiet.py")],
        slow=True,
        prepare=[["python3", str(ROOT / "scripts/test_native_harness_timeout.py")]],
    ),
]

BY_NAME = {scope.name: scope for scope in SCOPES}


def execute(command: list[str]) -> subprocess.CompletedProcess:
    if command and command[0] == "cargo" and "test" in command[:2]:
        return cargo_test(command, ROOT)
    return subprocess.run(
        command, cwd=ROOT, capture_output=True, text=True, errors="replace",
        env=running_here(),
    )


def running_here() -> dict:
    """Tell the tests which checkout they belong to.

    A path compiled into a binary is that of the checkout where it was built.
    Every worktree uses one cargo target, so one checkout can get a test
    binary built in another, and the tests would then read the other
    checkout's console packages.
    """
    return {**os.environ, "ROMINABOX_REPO": str(ROOT)}


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
            if scope.skipped:
                mark += f" (skipped unless named: {scope.skipped})"
            print(f"{scope.name}{mark}\n  covers    {scope.covers}\n  does not  {scope.not_covered}\n")
        return 0

    if arguments.scopes:
        unknown = [name for name in arguments.scopes if name not in BY_NAME]
        if unknown:
            raise SystemExit(f"unknown scope(s): {', '.join(unknown)}; try --list")
        selected = [BY_NAME[name] for name in arguments.scopes]
    elif arguments.all:
        selected = [scope for scope in SCOPES if not scope.skipped]
    else:
        selected = [scope for scope in SCOPES if not scope.slow and not scope.skipped]
    if not arguments.scopes:
        for scope in SCOPES:
            if scope.skipped:
                print(f"skipped {scope.name}: {scope.skipped}")

    SCRATCH.mkdir(parents=True, exist_ok=True)
    # We put this stamp in every scratch directory of this run. A diff of
    # $TMPDIR also shows other processes, so only names with this stamp are ours.
    scratch_run = f"{os.getpid()}-{time.time_ns()}"
    os.environ["ROMINABOX_SCRATCH_RUN"] = scratch_run
    # No test may use the network. While this is set, we refuse every download,
    # and we read a real core for a test from the local core source.
    os.environ["ROMINABOX_OFFLINE"] = "1"
    os.environ.setdefault(core_source.VARIABLE, str(core_source.core_source()))
    support_before = support_snapshot()
    temp_before = temp_snapshot()
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
    # `wall` is the limit for the fast selection. We do not apply it to a named
    # scope or to --all, because quitting Flycast once takes longer than the
    # whole fast suite.
    if arguments.all:
        wall_budget = (limits or {}).get("wall_all")
    elif arguments.scopes:
        wall_budget = None
    else:
        wall_budget = (limits or {}).get("wall")
    wall_ratio = f"  {wall / wall_budget:4.1f}x" if wall_budget else ""
    print(f"\nwall {wall:0.1f}s{wall_ratio}")
    if wall_budget and limits is not None and over_budget(wall, wall_budget, limits):
        slow.append("wall")
    support_after = support_snapshot()
    created = support_additions(support_before, support_after)
    modified = support_modifications(support_before, support_after)
    if created:
        print(
            "\nA test run created paths under the player's ROM-in-a-Box folders in ~/Library/Application Support:"
        )
        for path in created[:20]:
            print(f"  {path}")
        if len(created) > 20:
            print(f"  … and {len(created) - 20} more")
    if modified:
        print(
            "\nA test run modified paths under the player's ROM-in-a-Box folders in ~/Library/Application Support:"
        )
        for path in modified[:20]:
            print(f"  {path}")
        if len(modified) > 20:
            print(f"  … and {len(modified) - 20} more")
    leftover = [
        name for name in temp_additions(temp_before, temp_snapshot()) if scratch_run in name
    ]
    if leftover:
        print(
            f"\nA test run left {len(leftover)} entries in {temp_directory()} named rominabox*:"
        )
        for name in leftover[:20]:
            print(f"  {name}")
        if len(leftover) > 20:
            print(f"  … and {len(leftover) - 20} more")
    failed = [scope.name for scope in selected if not recorded[scope.name][0]]
    if failed:
        print(f"\n{len(failed)} scope(s) failed: {', '.join(failed)}")
    if created or modified or leftover or failed:
        return 1
    if slow:
        print(
            f"\n{len(slow)} timing(s) exceeded the budget in {BUDGETS.name}: {', '.join(slow)}\n"
            "A budget is the last measured time for that scope. It is exceeded "
            "when a run takes more than twice as long and at least the slack longer."
        )
    if not arguments.all and not arguments.scopes:
        print("\nSlow scopes were skipped. Run --all before a checkpoint.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
