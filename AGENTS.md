# Project guidance

Rules for anyone changing this repository, people and coding agents alike.

## The product

- The display name is **ROM-in-a-Box**. The repository and the command line are `rominabox`.
- Read `docs/product-notes.md` before product or architecture work.
- The builder is made with Tauri and a Rust backend. Neither the builder nor an exported game runs Python.
- The authoring flow starts with dropping or choosing a file, then shows the lookup progress, then the editable details. Player options are in the game's menu, in the chosen design. Routine emulator branding and notifications are hidden.
- Keep the interface concise: familiar controls, short labels, useful defaults and optional help.
- The person who bundles a game chooses its BIOS. A game has no BIOS screen, picker or upload for its player. The details step does not continue while a required file is missing, and the export is refused with the same words (`assess_firmware`).
- Make an export from a player-only template for the chosen design, never from the builder. It includes only the assets of that design, the core and its support files, the shared dependencies and the licence texts. Check the exported files themselves, because hiding an unused module in the interface is not enough.
- Size and launch time are acceptance criteria. A small cartridge game stays below 26 MB on disk (the `size` scope). We download cores when an export needs them and never bundle them with the builder. Measure the app on disk, with the runtime and the content separately, and audit transitive dependencies and upstream assets as well as our own modules.
- Every game keeps its configuration, saves, states, caches, history, overrides and credentials in its own managed storage. A game never reads another RetroArch installation or global defaults, on any platform, including the Windows registry.
- Windows and macOS are supported, and Linux is planned. Name every platform explicitly in code. There is no "otherwise it is Windows".

## Architecture

- Authoring features go through one shared engine, used by both the builder and the command line. Runtime integration, metadata lookup and platform packaging stay replaceable.
- Prefer declarative interfaces for capabilities and configuration, backed by one implementation. Avoid wiring that every caller repeats.
- **The menu code sets facts, and the designs set the look.** In the C++ in `vendor/retroarch/menu/drivers/rmlui` we set only generic state (`focused`, `selected`, `occupied`, `disabled`, and `data-focus` with the id of the focused element), and the RCSS of a design sets what is shown and when. Code in C++ that refers to a specific element to set its look belongs in the design instead. When a design cannot express something, add the smallest generic fact. Referring to an element to perform an action (SAVE saves) goes through the document contract.
- Values with a fixed set of members are enums or checked types, never bare strings or hand-computed numbers.
- Establish the capabilities of a core from the core itself: with `frame_harness --frames 1` you get the library name and the controller table of a core, and with `scripts/core_capabilities.py` its linked decoders.
- When a change runs into a bad structure, fix the structure. A comment that explains why two things must be kept in step means that they should be one thing.
- Designs use the shared contract of theme, state and actions. A design is not a recoloured copy of the layout of another design.

## Tests

- Outside-in workflow and integration tests are the foundation. Unit tests are welcome alongside them, for rules, edge cases and regressions. Do not replace our own components with mocks.
- A bug fix comes with a regression test, committed before the fix, which fails for the intended reason and passes after it. A refactor or a changed constant needs no failing test first.
- Prove a test against real code. Never commit a script that rewrites source or data to make a test fail, and never assert on source text.
- Look at the results: open the screenshots, and read the logs and the full outputs. Counts of passing tests come after that.
- Start from the smallest thing that runs and grow it, and test each assumption with an experiment whose result you can see.
- The tests are grouped in scopes. Run `uv run python scripts/test.py --list` for a description of each scope and what it does not prove, `uv run python scripts/test.py <scope>...` for some scopes, the same without arguments for the fast ones, and with `--all` for everything. While working, run the affected scopes. Add a test to the scope it belongs to, or add a scope.
- Automated runs never leave a window open. Check a core with `scripts/native_runtime/frame_harness.c` (no window, no audio, no RetroArch) or with RetroArch's `--max-frames` and `--max-frames-ss`, and run the player with `ROMINABOX_QUIET=1` (and `ROMINABOX_MENU_SHOT=1` for a picture). `video_driver=null` still creates a window briefly.
- Offscreen pictures and headless tests are no evidence of native appearance, focus or launch speed. Check those by hand in the built app.

## Tooling

- One Python runs every script and test, on every platform: uv's. `.python-version` declares the version and `uv.lock` pins the packages. Run scripts as `uv run python scripts/<script>.py`, never with a bare `python`. A package imported by a script goes into `pyproject.toml` and `uv.lock`. Child processes use `sys.executable`, which `scripts/test.py` passes on as `ROMINABOX_PYTHON`.
- No source file of ours grows past 1,000 lines. In `.githooks/pre-commit` we run `scripts/line_limit.py`. The files that are already longer are listed in `scripts/fixtures/line-limit.json`, and they may not grow.
- Do parallel work in worktrees made with `uv run python scripts/worktree.py create <suffix>`, each with its own dev port, builder identity, prefix for game identities and cargo target. Inside a worktree, run `eval "$(uv run python scripts/worktree.py env)"` before building or launching anything.
- RetroArch is our fork in `vendor/retroarch`. Make native changes there, commit the fork before moving this repository's pointer to it, and build from that commit.
- In scripts that delete files, write literal paths and check them first. Never pass a variable into a recursive delete.
- Listeners bind to 127.0.0.1, never to 0.0.0.0, and nothing requires administrator rights.

## Content and licences

- Keep games, firmware, saves, credentials and downloaded artwork out of Git.
- `licenses/` contains the licence text of every third-party component, for attribution (`scripts/licences.py`). Keep the provenance and notices of any dependency you add.
- Code comments describe the code as it is. They do not tell its history or the discussion behind it.
