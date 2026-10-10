"""Run one scope of the test suite, or all of them.

The tests are in five places, with five test runners, and while working you
rarely need all of them. After a change to a controller declaration you can
skip the frontend tests, and after a React change the ten overlay renders.

    uv run python scripts/test.py                 # the fast scopes
    uv run python scripts/test.py catalog         # one scope
    uv run python scripts/test.py catalog menu    # several
    uv run python scripts/test.py --all           # everything, including slow
    uv run python scripts/test.py --list          # what exists and what it covers

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
import player_build  # noqa: E402
import programs  # noqa: E402
import toolchain  # noqa: E402
from built import MANIFEST as ENGINE_MANIFEST  # noqa: E402
from cargo_replay import cargo_test, environment  # noqa: E402
from player_support import additions as support_additions  # noqa: E402
from player_support import modifications as support_modifications  # noqa: E402
from player_support import snapshot as support_snapshot  # noqa: E402
from player_support import user_data as support_user_data  # noqa: E402
from temp_entries import additions as temp_additions  # noqa: E402
from temp_entries import directory as temp_directory  # noqa: E402
from temp_entries import snapshot as temp_snapshot  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
SCRATCH = ROOT / "work/test-output"
BUDGETS = ROOT / "scripts/fixtures/scope-budgets.json"
PRINT_LOCK = threading.Lock()

PYTHON = programs.PYTHON

# The engine and the command line. There are no tests in the builder's window code.
CARGO_ENGINE = ["--manifest-path", str(ENGINE_MANIFEST)]
CARGO_CATALOG = ["--manifest-path", str(ROOT / "desktop/crates/rominabox-catalog/Cargo.toml")]
# bridge, dcmenu and menu link one RmlUi. We declare it in each scope that
# uses it, or in a checkout where nobody has compiled it the scope fails.
RMLUI_PREPARE = [[PYTHON, str(ROOT / "scripts/prepare_rmlui.py")]]

WORKFLOW_COMMAND = [PYTHON, str(ROOT / "scripts/menu_workflows.py")]


# What we link into a headless menu driver beside the cached menu objects,
# which is the fake RetroArch host, its keyboard and achievements stand-ins,
# and libretro's config reader. We build every driver the same way, so all of
# them run the same menu.
HEADLESS_FLAGS = ["--define", "HAVE_AUDIOMIXER"]
HEADLESS_SUPPORT = [
    "scripts/native_runtime/menu_host_fake.cpp",
    "scripts/native_runtime/text_test_host.cpp",
    "scripts/native_runtime/achievements_fake.cpp",
    "vendor/retroarch/libretro-common/file/config_file.c",
    "vendor/retroarch/libretro-common/file/config_file_io.c",
]


def headless_driver(output: Path, source: str) -> list[str]:
    """Return the harness helper's command to build one headless menu driver."""
    return [PYTHON, str(ROOT / "scripts/native_runtime/menu_harness.py"), "build",
            str(output), *HEADLESS_FLAGS,
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
        env: dict[str, str] | None = None,
        launches_games: bool = False,
        runs_player: bool = False,
    ):
        self.name = name
        self.covers = covers
        self.not_covered = not_covered
        self.command = command
        self.slow = slow
        # In this scope we start exported games. In the shot harness every game
        # is in one namespace (menu_shots.SHOT_BUNDLE_PREFIX), so two such scopes
        # at the same time could start the same game, with one identity,
        # sandbox, data folder and log. We run them one at a time, so in
        # launchtime we also never time a machine busy with other games.
        self.launches_games = launches_games
        # We run this checkout's test player here, built once before any scope
        # starts (player_build.selected_build), as in every scope that
        # launches a game.
        self.runs_player = runs_player or launches_games
        # What we must stage before the scope runs. We declare it here because
        # without the declaration, a scope with generated input would work only
        # in a checkout where someone had generated it, and fail everywhere else.
        self.prepare = prepare or []
        # Environment variables for the scope's command, added to this run's own.
        self.env = env or {}


SCOPES = [
    Scope(
        "catalog",
        "console packages parse, validate and generate the shipped registries",
        "that the generated data is correct for any real console, only that it is self-consistent",
        # We run every test file, so a failure in one cannot hide another.
        ["cargo", "test", "--quiet", "--no-fail-fast", *CARGO_CATALOG],
    ),
    Scope(
        "exporter",
        "the Rust exporter and player-facing declarations: staging, isolation, controls, capabilities, patches applied by RetroArch's own code, and that a command with no request does not read stdin",
        "that an exported game runs; every fixture core is a stand-in that is never loaded",
        ["cargo", "test", "--quiet", "--no-fail-fast", *CARGO_ENGINE],
        # In disc_layout and the other layout tests we measure and hover over a
        # composed menu through the RmlUi probe (tests/support), linked with RmlUi.
        prepare=RMLUI_PREPARE,
    ),
    Scope(
        "picture",
        "that a hard edge in a core's picture is still a hard edge after the options an export ships",
        "window placement, bilinear scaling of an already-sharp frame, or any console whose core is not in the kit",
        [PYTHON, str(ROOT / "scripts/picture_edges.py")],
    ),
    Scope(
        "shipped",
        "that the core options, remaps, controller profiles and firmware an export ships replace what an earlier export or data location left in the game's data on every launch, that a setting the player changed afterwards stays, that a player setting chosen in the game's menu reaches the next launch whatever default a later export carries, and that the launcher modules build for Windows",
        "that RetroArch reads those files or that the core honours them (the picture scope covers the core), or that the Windows build runs",
        [PYTHON, str(ROOT / "scripts/test_shipped.py")],
    ),
    Scope(
        "accounts",
        "the shared RetroAchievements accounts store on real files: who is listed, when an account leaves, private modes, unsafe names, several games changing it at once, and that it builds for Windows",
        "that a game calls it, that the sandbox lets a game reach the folder (the isolation scope), or that the Windows build runs",
        [PYTHON, str(ROOT / "scripts/test_accounts_store.py")],
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
        [PYTHON, str(ROOT / "scripts/builder_shots.py"), "--check"],
        slow=True,
    ),
    Scope(
        "menu",
        "what RmlUi does with the real menu.rml when clicked: hit testing, hover, focus, classes",
        "that the menu looks right, or anything about the C++ bridge, which is not loaded",
        [PYTHON, str(ROOT / "scripts/menu_interaction.py"), "--check"],
        prepare=RMLUI_PREPARE,
    ),
    Scope(
        "environment",
        "that the player reads a variable its launcher set as it was set: a value, a path with a non-ASCII name, an empty value, and no variable",
        "which variables the launcher sets, or what the player does with them",
        [PYTHON, str(ROOT / "scripts/test_environment_reader.py")],
    ),
    Scope(
        "heldkey",
        "that Escape still toggles the menu while another key is held, including a press that starts and ends between two samples, that Alt+Enter toggles fullscreen, and how we split the brightness the player chose between a shader's brightness parameter and our pass",
        "that a physical keyboard delivers the events; the decision is the function the runloop calls",
        [PYTHON, str(ROOT / "scripts/test_held_key.py")],
    ),
    Scope(
        "gamedata",
        "the zips of a game's data, from the C sources the player and the launcher compile: a game's data exported, set aside as the menu does for an import, and imported as the launcher does before the next launch, with saves renamed to another game's file, the game's own states replaced, its settings kept and its login left out, and that it builds for Windows",
        "the menu's EXPORT DATA and IMPORT DATA, the file panels or the restart; the builder's checks of untrusted zips are in the exporter scope, through the engine",
        [PYTHON, str(ROOT / "scripts/test_game_data.py")],
    ),
    Scope(
        "typing",
        "that in the menu the keyboard presses only the arrows of the menu's pad: no key the game's controls hold, none of RetroArch's other menu keys, no key typed into the text entry and no key a hotkey of the menu is bound to, while the game's keys are its buttons as it plays; RetroArch's own function that reads the keyboard for the menu, handed a held key",
        "whether the menu says it is typing or which keys its hotkeys hold (the navigation and bridge scopes ask the real menu), or that a physical keyboard delivers the key; the rest of RetroArch is stand-ins that stop the program if reached",
        [PYTHON, str(ROOT / "scripts/test_menu_typing.py")],
    ),
    Scope(
        "menupad",
        "that in the menu a controller navigates by its profile: the d-pad's own Left is the menu's Left after the game's controls moved Left to another button, which is no button of the menu, while that button is the game's Left as it plays; RetroArch's own function that reads the menu's pad, handed a held button",
        "a physical controller, or the menu reacting to the press (the navigation scope); the joypad driver and the rest of RetroArch are stand-ins",
        [PYTHON, str(ROOT / "scripts/test_menu_pad.py")],
    ),
    Scope(
        "lastpad",
        "which pad a core's rumble reaches, from RetroArch's own rumble interface and pad reading in the fork: with every pad player 1, the pad that last pressed a button, the pad it leaves told 0 for both motors and the new one the strengths the core set, gain applied once, with no move for a held button, a stick or the keyboard, and no rumble for a player no pad plays as; with one pad per player, the pad RetroArch gave each player, as upstream, whatever is pressed",
        "that a physical pad rumbles or stops when told 0 (the joypad driver is a stand-in), or that an export plays every pad as player 1 (the exporter scope reads that through the remap loader and the input layer)",
        [PYTHON, str(ROOT / "scripts/test_last_pad.py")],
    ),
    Scope(
        "sdlrumble",
        "what RetroArch's SDL2 joypad driver in the fork, the Mac player's, tells SDL when a core sets a pad's two motors one after the other: both strengths as they now are, each call, and that it answers a rumble SDL made as made",
        "that a physical pad rumbles (SDL stands in, with one pad that rumbles through SDL_JoystickRumble and no haptic device), or SDL's haptic path",
        [PYTHON, str(ROOT / "scripts/test_sdl2_rumble.py")],
    ),
    Scope(
        "staging",
        "that the runtime-kit staging script names paths that exist, after any rename, and that this checkout's kit carries a launch library built from the launcher's sources as they are",
        "that the script runs or produces a correct kit; it builds a whole application",
        [PYTHON, str(ROOT / "scripts/test_staging.py")],
    ),
    Scope(
        "glslcore",
        "that a core OpenGL context is given a GLSL version it accepts, and that a missing shader path is not passed to path_basename",
        "that a real context compiles the stock shader; it compiles the decision from the fork and hands it versions",
        ["node", str(ROOT / "scripts/native_runtime/test_glsl_core.mjs")],
    ),
    Scope(
        "dcmenu",
        "that a core-profile context draws the menu and a legacy context still does, that each loads a picture from a folder with a non-ASCII name through libretro's file layer, that each draws text made after a game's frame left the unpack row length set as it was given, that each draws into the framebuffer bound as the window (DXGI's, in a fullscreen Windows game) and leaves it bound, that each draws an offset and an inset box-shadow through RmlUi, and that a log line reaches the file before the process exits",
        "that a Dreamcast disc boots, where the menu sits, or that a Windows path is read; the pictures are a separate run",
        [PYTHON, str(ROOT / "scripts/test_dcmenu.py")],
        prepare=RMLUI_PREPARE,
    ),
    Scope(
        "joypad",
        "that every hid profile the pin declares is staged, and that RetroArch's match rules would accept it",
        "that a physical pad's buttons match those numbers; nothing here opens a device",
        [PYTHON, str(ROOT / "scripts/test_joypad_autoconfig.py")],
    ),
    Scope(
        "preparation",
        "that preparing a core or the controller profiles stages them whatever their licence texts say, naming a missing or changed text in a warning",
        "that a real source archive or nightly holds what it should; every archive is made in a temporary folder and nothing reaches the network",
        [PYTHON, str(ROOT / "scripts/test_preparation.py")],
    ),
    Scope(
        "licences",
        "that licenses/ holds a current entry for every third-party component: the player's libraries, the cores, the crates, the builder's npm packages, the fonts and the data the product carries; and that a missing, changed or unused entry, or a player build compiling a fork library with no entry, is named in a warning that fails nothing",
        "that a text read from the network is still what its URL serves, or that a component's declared licence is right; it reads no network, and the real player build only when build_kit.py makes a kit",
        [PYTHON, str(ROOT / "scripts/test_licences.py")],
    ),
    Scope(
        "padrelay",
        "on Windows, that a sandboxed game lists the controllers DirectInput lists outside, again after letting go of them as the joypad driver does when one comes or goes, sets one up and reads it within the range it set, asks for rumble through its launcher, is refused whatever it writes into the relay, and loses its controllers when the launcher stops answering; elsewhere, with zig, that both sides build for Windows",
        "RetroArch's joypad driver using it (the isolation scope sees a gamepad), a controller physically plugged in or out, or rumble on a controller that can; without a controller connected, anything about one",
        [PYTHON, str(ROOT / "scripts/test_pad_relay.py")],
    ),
    Scope(
        "reporoot",
        "that no test or script uses a place in one person's home, that every script building against RmlUi uses the declared one and the picture scopes the packaged preview renderer, and that no script or test names the removed experiment tree",
        "that a place it accepts holds what a test needs; it reads how each place is named and asks each script what it uses",
        [PYTHON, str(ROOT / "scripts/test_repo_root.py")],
        # In it we ask the menu harness what it compiles with, FreeType included.
        prepare=RMLUI_PREPARE,
    ),
    Scope(
        "fixtures",
        "that a test file this repository does not generate is fetched or skipped out loud, and that the generated cartridge is ready",
        "that a fetched disc boots; the quit scope launches one, and only when the file is actually there",
        [PYTHON, str(ROOT / "scripts/test_fetch_content.py")],
    ),
    Scope(
        "symlinks",
        "that git carries no symbolic link, which would point somewhere else on every other machine",
        "that a worktree has the links it needs, or that the ignore rules are right",
        [PYTHON, str(ROOT / "scripts/test_no_symlinks.py")],
    ),
    Scope(
        "worktree",
        "isolation between parallel checkouts: the shared git dir, the lock, refusing the canonical tree, each checkout's own cargo target, 15 GB free before one is made, and that create will not check out an existing branch",
        "that a real worktree builds or runs; it creates nothing outside a temporary directory",
        [PYTHON, str(ROOT / "scripts/test_worktree.py")],
    ),
    Scope(
        "shotsign",
        "that replacing the shot player keeps the sandbox the export signed, and that every shot shares one bundle namespace",
        "that a picture was taken; that is menu_shots, and this does not launch a game",
        [PYTHON, str(ROOT / "scripts/test_shot_sign.py")],
    ),
    Scope(
        "stagedkit",
        "that the kit a launched test exports from carries the tree's shared menu parts and shader library, and that a second kit or a second plan tool compiles the game's launcher no more",
        "that a game exported from the kit runs (the launched scopes), or that a change to the launcher's sources is compiled; ninja compiles a Windows launcher again when its inputs change, and a macOS launch library is built again when the digest of its sources changes",
        [PYTHON, str(ROOT / "scripts/test_staged_kit.py")],
    ),
    Scope(
        "publishkit",
        "that a kit packed for publishing is named <platform>-<player>.zip with the kit's files at its root, that an archive whose name and manifest disagree is rejected before upload, and that the target repository is the one in the engine package's Cargo.toml",
        "that an upload reaches GitHub, or the download itself, which is tested in kits.rs with a stand-in transport",
        [PYTHON, str(ROOT / "scripts/test_publish_kit.py")],
    ),
    Scope(
        "achievement-client",
        "managed account, evaluator, pending uploads and state restoration using the real rcheevos client",
        "RetroArch core-memory mapping or its actual runloop/HTTP/save-task adapters",
        [PYTHON, str(ROOT / "scripts/test_achievements_client.py")],
    ),
    Scope(
        "achievement-native",
        "actual exported core/client authentication, autosave restoration, unlock, OFF and exclusion against a loopback service; the signed-in game in its sandbox on macOS, outside it on Windows, whose sandbox cannot reach loopback (SANDBOX_REACHES_LOOPBACK)",
        "a real RetroAchievements account, physical keyboard/controller behavior, or on Windows the sandbox's part in signing in (the isolation scope and a hand check cover that)",
        [PYTHON, str(ROOT / "scripts/achievements_native_workflow.py")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "account-input",
        "real RmlUi account input, Unicode/composition, logical navigation, notification lifetime and both design/palette combinations",
        "physical dead keys, native IME candidate placement, controllers or live login",
        ["sh", str(ROOT / "scripts/achievements_input_test.sh")],
        # The checks of the harness build itself. With a stale object from the
        # cache, bridge would compile against code that no longer exists.
        prepare=RMLUI_PREPARE + [[PYTHON, str(ROOT / "scripts/native_runtime/test_menu_harness.py")]],
    ),
    Scope(
        "bridge",
        "the real menu and RmlUi document with a dummy renderer: actions, shared pointer/key focus, binding capacity, capture lifecycle, save/load failures and persistence",
        "physical input capture or audible sound; a fake RetroArch host controls the failure/capture boundary, and rendering appearance needs direct screenshot review",
        [PYTHON, str(ROOT / "scripts/native_runtime/test_rmlui_interaction.py")],
        # We build bridge through the same cache, so we check the cache there too.
        prepare=RMLUI_PREPARE + [[PYTHON, str(ROOT / "scripts/native_runtime/test_menu_harness.py")]],
    ),
    Scope(
        "navigation",
        "arrow keys, pointer, focus and their sounds on every screen of every registered design and of the hypothetical layouts, driven through the real menu C++ on composed documents, and that a save slot's picture takes the game's shape where a design marks it",
        "physical keyboards, pads or mice, audible sound, or how a highlight looks; a fake RetroArch host stands in for the player and nothing is drawn",
        ["cargo", "test", "--quiet", *CARGO_ENGINE, "--test", "menu_navigation", "--", "--include-ignored"],
        prepare=RMLUI_PREPARE + [headless_driver(NAVIGATION_DRIVER, "scripts/native_runtime/menu_nav_driver.cpp")],
        env={"ROMINABOX_NAVIGATION_DRIVER": str(NAVIGATION_DRIVER)},
    ),
    Scope(
        "workflows",
        "every menu workflow case (keys, pointer, controls, capture, volume, shaders, saves, overlays, accounts) in both designs and every palette, replayed through the fork's own script driver and report on menus composed as an export composes them, compared checkpoint by checkpoint and file by file with what the launched player recorded",
        "anything drawn, audible cues, RetroArch's bind descriptions and remap files, or physical input; the fake host stands in for RetroArch, and the workflows-native scope launches a few cases for real",
        ["cargo", "test", "--quiet", *CARGO_ENGINE, "--test", "menu_workflows", "--", "--include-ignored", "--nocapture"],
        prepare=RMLUI_PREPARE + [headless_driver(WORKFLOW_DRIVER, "scripts/native_runtime/menu_workflow_driver.cpp")],
        env={"ROMINABOX_WORKFLOW_DRIVER": str(WORKFLOW_DRIVER)},
    ),
    Scope(
        "workflows-native",
        "the few workflow cases menu-workflows.json marks `launched`, in the exported player itself: one per design and a save and load through RetroArch's own state task, compared with the same baselines as the headless replay, and each picture",
        "audible cues, physical input or native focus/fullscreen; inspect the captured images directly too",
        WORKFLOW_COMMAND + ["--output", str(SCRATCH / "menu-workflows")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "hotkeys",
        "in the exported test player, that QUICK SAVE writes the slot the menu has selected, its state and its picture, that PREVIOUS SLOT and NEXT SLOT step it round the six slots, that QUICK LOAD of an empty slot loads nothing and of a saved one loads it, that each says so in the notice row, that the menu shows the slot they chose, and that the next launch starts on it",
        "that a physical keyboard's press reaches the menu (the script holds the key where the menu reads the keyboard), sound, or window focus; the screen's own rules are the bridge scope's",
        [PYTHON, str(ROOT / "scripts/test_play_hotkeys.py"), str(SCRATCH / "play-hotkeys")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "cleanup",
        "on Windows, that a launched test whose real export fails part-way, as the game is built, while it is open, or in the shipped scope's menu sounds case, leaves nothing of the game in the person's ROM-in-a-Box folders, no sandbox and no temporary folder",
        "that a game plays (no player starts: each launch is plan-only), or anything on macOS, whose games unpack nothing and keep their data in their containers",
        [PYTHON, str(ROOT / "scripts/test_launched_cleanup.py")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "player",
        "that the built player refuses to start without an absolute data folder, with one starts and creates nothing beside itself, and on Windows declares UTF-8 as its code page; and that a header gone from the fork since the build folder was built does not stop its next build, for each kind of source it compiles, and that moving a build folder to a commit that rewrote a source in another language leaves no object of the old one",
        "where a game's folders go once it runs; it only asks the player for its feature list, before any window or core. The header check asks make what it would do, and does not build",
        [PYTHON, str(ROOT / "scripts/test_player.py")],
        slow=True,
        runs_player=True,
    ),
    Scope(
        "edges",
        "that a photographed open list and a photographed focused control have all four outline edges painted",
        "where the list was placed, or that the boxes in the bridge agree; it only reads the picture",
        [PYTHON, str(ROOT / "scripts/check_menu_edges.py")],
    ),
    Scope(
        "rings",
        "that a lit ring stands out from a pale pad all the way round: GameCube's Control stick hovered and focused, in every design",
        "where the ring is (menu_scene in the exporter scope asks that), or how a ring looks on any other pad",
        [PYTHON, str(ROOT / "scripts/check_ring_contrast.py"), str(SCRATCH / "ring-contrast")],
    ),
    Scope(
        "pictures",
        "that a badge still downloading draws a moving placeholder and a failed one a mark, in every design, and draws every Disc screen into work/feedback-pictures",
        "that the player sets those classes or keeps redrawing: the rows are written the way its list writes them, and the second moment is RmlUi's 0.1 s step",
        [PYTHON, str(ROOT / "scripts/menu_pictures.py")],
    ),
    Scope(
        "placement",
        "that the controller picker lands in the same place on every console that offers one",
        "that the place is a good one — only that it is the same one, whichever pad is drawn",
        [PYTHON, str(ROOT / "scripts/menu_states.py"), "--fixed-place", str(SCRATCH / "picker-place")],
        slow=True,
    ),
    Scope(
        "states",
        "that every declared state of the menu draws, in every design and palette, on the screen it names, and that no two states draw the same picture, so a player can tell each one apart",
        "that a state looks good, or that the player reaches it at the right moment; for that a person looks at the pictures `python scripts/menu_states.py work/menu-states` draws",
        [PYTHON, str(ROOT / "scripts/menu_states.py"), str(SCRATCH / "menu-states")],
        slow=True,
    ),
    Scope(
        "variants",
        "that every controller a player can pick has artwork staged and a scene to swap to",
        "that the player actually swaps to it; that is the native menu, which is not linked here",
        [PYTHON, str(ROOT / "scripts/menu_states.py"), "--every-variant", str(SCRATCH / "variants")],
        slow=True,
    ),
    Scope(
        "identification",
        "how many catalogue names get the correct cover, against every published picture list",
        "that every cover downloads, or that a real ROM was hashed; it matches names to the published filenames and checks one pointer file",
        [
            "cargo",
            "test",
            *CARGO_ENGINE,
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
        "that something other than a person still runs this suite before a push, and the line limit before a commit",
        "that the hook is installed in a fresh clone; core.hooksPath is local configuration",
        [PYTHON, str(ROOT / "scripts/test_automation.py")],
    ),
    Scope(
        "linelimit",
        "that the pre-commit check stops a source file over the line limit unless it is listed, a listed file that grows, and a list that no longer matches its files, reading what is staged",
        "that the hook is installed (the automation scope reads that it is wired), or anything about this repository's own files",
        [PYTHON, str(ROOT / "scripts/test_line_limit.py")],
    ),
    Scope(
        "icons",
        "that every ROM-in-a-Box icon (the builder's .icns, .ico and header picture, a game's icon without artwork, and the splash a game shows at startup) still matches a fresh render of logo.svg",
        "that the icon looks right, or how macOS draws it; only that the files have not diverged from the drawing",
        [PYTHON, str(ROOT / "scripts/render_icons.py"), "--check"],
        slow=True,
    ),
    Scope(
        "artwork",
        "that every controller PNG still matches a fresh render of its SVG source",
        "that the artwork is correct — only that the PNG has not diverged from the drawing",
        [PYTHON, str(ROOT / "scripts/render_controllers.py"), "--check"],
        slow=True,
    ),
    Scope(
        "menupreview",
        "that the builder can draw its own preview of every design it offers, in every palette, scaled to the picture as the player scales it to the window, with a text change drawn as markup",
        "what the menu looks like — the states scope asks that; this asks whether the preview draws it at all, and as the player would",
        [PYTHON, str(ROOT / "scripts/test_menu_preview.py")],
        slow=True,
    ),
    Scope(
        "shaderpreview",
        "that every shader's preview is still what that shader does to a picture, rendered from its own GLSL",
        "that the filter is a good one — only that the picture of it is made by running it",
        [PYTHON, str(ROOT / "scripts/render_shader_previews.py"), "--check"],
        slow=True,
    ),
    Scope(
        "size",
        "that the space an exported app takes on disk, as the file system allocates it (a Windows game unpacked, then forgotten by its own UNINSTALL), stays under the size ceiling, and that it carries no library but its core",
        "a cartridge's own size, or that the player was rebuilt; it measures the kit already on disk",
        [PYTHON, str(ROOT / "scripts/size_bundles.py")],
    ),
    Scope(
        "isolation",
        "that a signed export keeps the sandbox entitlement, cannot read or write the player's RetroArch profile or another game's container, and still loads a core, stays quiet, and sees a gamepad",
        "window placement, focus, fullscreen, or that Gatekeeper accepts an ad-hoc signature",
        [
            "cargo",
            "test",
            *CARGO_ENGINE,
            "--test",
            "isolation",
            "--",
            "--ignored",
            "--nocapture",
        ],
        slow=True,
    ),
    Scope(
        "wingame",
        "on Windows, a game made into one program, opened as a person opens it, with the launcher built from "
        "this tree and a stand-in player: its first launch unpacks it whole, a second runs that copy without "
        "unpacking it again, a file past 260 characters while it is unpacked still unpacks, a newer version "
        "unpacks beside the older copy, which goes while the saves stay, and UNINSTALL removes the game's data, "
        "its sandbox, what it kept before it had a sandbox and every unpacked copy, and keeps the program; and "
        "a program whose index names a folder outside the runtimes folder is refused, writing and removing "
        "nothing; on macOS nothing, since a Mac game is not one program",
        "the unpacking dialog, a real player or core, UNINSTALL chosen in a running game's menu (the forget "
        "scope), or a disk that fills while a game unpacks",
        ["cargo", "test", "--quiet", *CARGO_ENGINE, "--test", "windows_game", "--", "--ignored"],
    ),
    Scope(
        "overlays",
        "that no controller callout or button anchor moved, across every illustrated profile",
        "that the positions are correct — only that they are unchanged since a human looked",
        [PYTHON, str(ROOT / "scripts/render_control_overlays.py"), "--check", str(SCRATCH / "overlays")],
        slow=True,
    ),
    Scope(
        "shaderstate",
        "that the shader row marked ON is the preset the running game is using, including after a restart",
        "that the filter looks right — only which row says it is the one on",
        [PYTHON, str(ROOT / "scripts/shader_state.py")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "discs",
        "that a cartridge and a single disc are not a multi-disc game, that choosing the second image makes the core report that index, and that an exported game's menu does the same in both designs while a one-disc game hides the Disc entry and opens the circle",
        "that a disc name was shortened — the bridge measures that",
        [PYTHON, str(ROOT / "scripts/test_discs.py")],
        slow=True,
        # In it we lay the menu out with RmlUi to find the Disc entry to click.
        prepare=RMLUI_PREPARE,
        launches_games=True,
    ),
    Scope(
        "quit",
        "that quitting an exported game unloads the core before the process exits: an Apple Event "
        "on macOS, closing the window on Windows, whose window also names the game's program for a "
        "pinned taskbar button",
        "window placement and fullscreen; on macOS closing the window is the same AppKit terminate "
        "path; a quit that asks first (unsent achievements); that the taskbar honours the labels",
        [PYTHON, str(ROOT / "scripts/test_quit.py")],
        slow=True,
        prepare=[[PYTHON, str(ROOT / "scripts/fetch_test_content.py"), "--scope", "quit"]],
        launches_games=True,
    ),
    Scope(
        "launchtime",
        "how long an exported game takes from starting to its first frame (an upper bound: a "
        "one-frame run, teardown included), first run and warm, on this platform",
        "a budget (none is set yet); a real game's core or content; a visible window",
        [PYTHON, str(ROOT / "scripts/test_launch_time.py")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "dxgi",
        "on Windows, that a fullscreen game's picture presented through DXGI shows upright, as Windows composes "
        "its window, and follows the window when it changes size; macOS presents through AppKit and has nothing here",
        "that the display stays in HDR (checked by hand) or that the player uses the "
        "presenter when it goes fullscreen",
        [PYTHON, str(ROOT / "scripts/test_wgl_dxgi.py")],
    ),
    Scope(
        "scripted",
        "that a run a menu script drives starts no controller driver and a run without one starts "
        "the platform's, whichever pads are plugged in",
        "that the script does anything (workflows-native), or that a physical pad works",
        [PYTHON, str(ROOT / "scripts/test_scripted_run.py")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "splash",
        "that a game with the splash waits while it is up, as long as the design declares and not much more: "
        "in a quiet run of the exported test cartridge, the time from the menu's first frame to the menu "
        "opening in a game that opens at its menu",
        "what the splash looks like or that a window shows it (a quiet run's window is hidden), or how long a "
        "person's launch takes to draw its first frame",
        [PYTHON, str(ROOT / "scripts/test_splash_hold.py")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "forget",
        "that UNINSTALL on Windows and RESET on macOS, chosen in a running game's menu by the menu's script "
        "driver, remove what the game keeps and keep the game: on Windows its sandbox, registered and with the "
        "game's data, and its unpacked copy, the program staying; on macOS its data folder, the app staying",
        "that a person's pointer or keys reach the button (the navigation scope), the screen's words, or the "
        "builder's uninstaller; RESET has not been run on a Mac yet",
        [PYTHON, str(ROOT / "scripts/test_forget_in_menu.py")],
        slow=True,
        launches_games=True,
    ),
    Scope(
        "quiet",
        "that a launch is quiet exactly when the switch is set, null audio, transparent windows, and safe native timeout handling",
        "actual GL presentation or hands-on focus/fullscreen; the window probe never orders its window in",
        [PYTHON, str(ROOT / "scripts/test_quiet.py")],
        slow=True,
        prepare=[[PYTHON, str(ROOT / "scripts/test_native_harness_timeout.py")]],
    ),
]

BY_NAME = {scope.name: scope for scope in SCOPES}


# What we add to this process's environment for every program of a run, which
# is the Python of this run, for the programs that are not Python (the Rust
# tests' repo::python(), scripts/python.mjs).
RUN_ENVIRONMENT = {"ROMINABOX_PYTHON": PYTHON}


def execute(command: list[str], env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    """`command`, given the run's environment and the scope's `env`."""
    added = {**RUN_ENVIRONMENT, **(env or {})}
    if command and command[0] == "cargo" and "test" in command[:2]:
        return cargo_test(command, ROOT, added)
    program = command[0] if Path(command[0]).is_absolute() else programs.find(command[0])
    if program is None:
        return subprocess.CompletedProcess(command, 127, "", f"{command[0]} is not on PATH\n")
    return subprocess.run(
        [program, *command[1:]], cwd=ROOT, capture_output=True, text=True, errors="replace",
        env=environment(added), **programs.windowless(),
    )


def run(scope: Scope) -> tuple[bool, float, str]:
    started = time.monotonic()
    chunks: list[str] = []
    for step in scope.prepare:
        staged = execute(step, scope.env)
        chunks.append(staged.stdout or "")
        chunks.append(staged.stderr or "")
        if staged.returncode != 0:
            chunks.append(f"  could not stage what {scope.name} needs\n")
            return False, time.monotonic() - started, "".join(chunks)
    result = execute(scope.command, scope.env)
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

    # In every scope we compile with one toolchain and run programs built with it.
    absent = toolchain.missing()
    if absent:
        raise SystemExit(
            "the native toolchain is not installed where expected: "
            + ", ".join(str(directory) for directory in absent)
            + " (set ROMINABOX_MSYS2 to the MSYS2 installation)"
        )
    toolchain.activate()

    if arguments.scopes:
        unknown = [name for name in arguments.scopes if name not in BY_NAME]
        if unknown:
            raise SystemExit(f"unknown scope(s): {', '.join(unknown)}; try --list")
        selected = [BY_NAME[name] for name in arguments.scopes]
    elif arguments.all:
        selected = list(SCOPES)
    else:
        selected = [scope for scope in SCOPES if not scope.slow]

    SCRATCH.mkdir(parents=True, exist_ok=True)
    # We put this stamp in every scratch directory of this run. A diff of
    # $TMPDIR also shows other processes, so only names with this stamp are ours.
    scratch_run = f"{os.getpid()}-{time.time_ns()}"
    os.environ["ROMINABOX_SCRATCH_RUN"] = scratch_run
    # No test may use the network. While this is set, we refuse every download,
    # and we read a real core for a test from the local core source.
    os.environ["ROMINABOX_OFFLINE"] = "1"
    os.environ.setdefault(core_source.VARIABLE, str(core_source.core_source()))
    # We build it once, before any scope starts, and give its path to every
    # scope in ROMINABOX_TEST_BUILD, so we never build it again in a scope.
    if any(scope.runs_player for scope in selected):
        print("building the test player", flush=True)
        built = time.monotonic()
        player_build.selected_build()
        print(f"test player {os.environ['ROMINABOX_TEST_BUILD']} ({time.monotonic() - built:0.1f}s)", flush=True)
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

    # Longest first, by each scope's last recorded time, so that no long scope
    # runs alone at the end. We run the scopes that launch games one at a time
    # in a separate queue, beside the others, so they do not occupy a worker
    # while they wait. We run four others at a time. With more, the renders and
    # the test binaries wait for each other and the run is no faster. We run
    # Cargo one at a time in cargo_replay, because the target directory is
    # shared.
    measured = (load_budgets() or {}).get("scopes") or {}
    longest_first = sorted(selected, key=lambda scope: -measured.get(scope.name, 0.0))
    games = [scope for scope in longest_first if scope.launches_games]
    others = [scope for scope in longest_first if not scope.launches_games]

    def game_lane() -> None:
        for scope in games:
            finish(scope)

    workers = min(4, len(others), os.cpu_count() or 4) or 1
    with ThreadPoolExecutor(max_workers=workers + 1) as pool:
        pending = [pool.submit(game_lane)] if games else []
        pending += [pool.submit(finish, scope) for scope in others]
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
            f"\nA test run created paths under the player's ROM-in-a-Box folders in {support_user_data()}:"
        )
        for path in created[:20]:
            print(f"  {path}")
        if len(created) > 20:
            print(f"  … and {len(created) - 20} more")
    if modified:
        print(
            f"\nA test run modified paths under the player's ROM-in-a-Box folders in {support_user_data()}:"
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
