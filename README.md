# ROM-in-a-Box

Turn a game you own into a standalone game app. Drop the game into the builder, review the details we detect, choose the look of its menu, and export an app that runs on its own. Save an authoring project to keep the game, artwork and settings together for later edits.

Website: <https://www.rominabox.app>

The builder runs on macOS and Windows and makes games for macOS (Apple silicon, and Intel when asked), for Windows, or for both at once in one zip. A game for the other platform needs the runtime kit of that platform ([below](#games-for-the-other-platform)). We can identify more systems than we can export. Linux is planned.

## Builder

The builder is made with Tauri, React and a shared Rust engine. Neither the builder nor an exported game needs Python.

The authoring flow is **Game → Details → Menu → Export**. For the metadata lookup we check established game catalogs and fetch matching artwork when there is any. A supported game with no match is still usable. For artwork we send the matched title to GitHub, and the bytes of the game stay on the computer. You can turn the lookup off under Advanced.

With **Save project** you write a `.rominabox` archive with the game and the chosen pictures, without emulator binaries, player saves or credentials. With **Open project** on the first screen you restore the settings and pictures, wherever the original files are. Before replacing an existing app, we ask at export.

The in-game menu has six save slots and Continue, Save, Load and Quit, and Options with controls, hotkeys, shader filters, achievements, a disc list for multi-disc games and the player's settings. There are three menu designs (ROM-in-a-Box, Pixel and Disc) and five colour palettes. Menu sounds are optional, from original procedural packs. In the builder's preview and in the game we draw the menu with the same RmlUi renderer.

Under **About** in the builder is every component of the builder and its games, with each licence text. The components of a game are under Options, ABOUT, in the game.

### Games for the other platform

A game is made from a runtime kit: the player and the files that ship with it, for one platform. The builder contains the kit of its own platform. A kit can be built only on its own platform: the Windows player with MSYS2 on Windows, and the Mac player with Xcode's tools on a Mac. The export itself runs anywhere.

The first time an export needs the kit of the other platform, we download it from the archive pinned in `desktop/kits.json` for its player (the fork commit of its RetroArch). No kit archive is published yet. Until then, an export for the other platform stops with an error that gives the folder for the kit: `<platform>-<first 12 characters of the fork commit>` in the builder's kit store, `~/Library/Application Support/<builder id>/kits` on macOS and `%LOCALAPPDATA%\<builder id>\kits` on Windows, where the builder id is `com.rominabox.desktop`. Copy the `desktop/src-tauri/resources/runtime` of the other machine there, made with `scripts/build_kit.py` from the same fork commit as the player of this builder. We accept no kit from any other commit.

To publish a kit, pack it with `uv run python scripts/pack_kit.py KIT ARCHIVE.zip`, which also prints the entry for `desktop/kits.json`, to fill in once the archive has a URL.

## Run and build

To work in a browser, install Node.js and run this from `desktop/`:

```sh
npm ci
npm run dev
```

Open `http://127.0.0.1:1420/`. In the preview you can open a local file and go through the flow. Metadata lookup, project archives and export run in the desktop app. Changes to the web interface update without rebuilding RetroArch.

Native builds need Rust and Tauri's build prerequisites, plus a prepared runtime kit in `desktop/src-tauri/resources/runtime` and the offscreen menu renderer in `desktop/src-tauri/resources/preview`. These are generated and not tracked: see [the native runtime](docs/native-runtime.md). The player of the macOS kit is universal (arm64 and x86_64) and links only system libraries. At export we create icons and zips in Rust and sign with the system's codesign, without Homebrew, Xcode or administrator rights.

```sh
cd desktop
npm run build
cargo check --workspace
cd ..
uv run python scripts/built.py --build
```

The Rust code is one Cargo workspace, `desktop/Cargo.toml`: the engine and `rominabox-cli` in `desktop/crates/rominabox-engine`, and the builder's window in `desktop/src-tauri`. With `scripts/built.py --build` we build `rominabox-cli` in the same way as in the builder build, so both share one compile of the engine.

With `uv run python scripts/build_builder.py` you build the builder with its kit and menu renderer (`scripts/build_player.py`, then `scripts/build_kit.py`): a signed `.app` on macOS and an installer on Windows. No builder or game is started.

Developer scripts and tests run with one Python on every platform, through [uv](https://docs.astral.sh/uv/): the version in `.python-version`, with the packages pinned in `uv.lock`. They are fetched at the first `uv run python scripts/<script>.py`. Run `uv run python scripts/test.py <scope>` for one scope of the tests, with `--list` for what each scope covers and leaves out, and with `--all` for everything, as in the pre-push hook once you install the hooks with `git config core.hooksPath .githooks`. For the tests that run the player, we build a test player from the fork.

## Command line and agent interface

The builder and `rominabox-cli` share identification, pictures, the menu preview, project archives and export: everything in the builder is also in the command line, through the same Rust functions. Commands take JSON on stdin, with JSON Lines on stdout as output, and the interface is described by `--help` and `schemas`. In a built builder, the command line is at `ROM-in-a-Box.app/Contents/Resources/bin/rominabox-cli` on macOS, and on the PATH after installing on Windows.

In the [agent skill](plugin/skills/rominabox/SKILL.md) we describe how an agent can use `rominabox-cli`: requests, defaults, outputs and limits. To install it in Claude Code from this repository, run `/plugin marketplace add Jorl17/rominabox`, then `/plugin install rominabox@rominabox`.

## Games

A game keeps its saves, states, caches, logs and configuration in its own folder: `~/Library/Application Support/ROM-in-a-Box/Games/<identity>` on macOS and `%LOCALAPPDATA%\ROM-in-a-Box\Games\<identity>` on Windows. The identity comes from the game's content and console, so renaming or moving an app keeps its saves. A game never reads the settings of another RetroArch installation. On Windows, a game runs in an AppContainer sandbox, unpacked into `%LOCALAPPDATA%\ROM-in-a-Box\Runtimes`.

Press **Esc** during play for the menu. The menu is a RetroArch and RmlUi integration in our fork of RetroArch, not an upstream RetroArch feature.

## Licences

The licence text of every third-party component is in `licenses/`, written with `scripts/licences.py`, with an index in `licenses/index.json`. A runtime kit contains the entries for what its games can ship, and every exported game contains its own, listed on its ABOUT screen. Under its licence, Genesis Plus GX may not be used commercially. The licence of ROM-in-a-Box itself is not chosen yet.

Keep games, firmware, saves, downloaded artwork and saved projects out of Git.

An exported cartridge game is meant to stay below 26 MB on disk, and we check this in the `size` test scope. We download cores when an export needs them and do not bundle them with the builder.

See also [engineering](docs/engineering.md) and [product principles](docs/product-notes.md).

## Native source checkout

RetroArch is maintained in our fork, [rominabox-retroarch](https://github.com/Jorl17/rominabox-retroarch), pinned here as `vendor/retroarch`. Clone with `git clone --recurse-submodules`, or run `git submodule update --init --recursive` after cloning. Native changes go into the fork: commit there first, then commit the updated pointer here. See [the native runtime](docs/native-runtime.md).
