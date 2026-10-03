# Native menu runtime

The player is our fork of RetroArch, the submodule at `vendor/retroarch`, with
RmlUi pinned at `ba95ffe8bfb6370efb2cdcca927eaad4710c5413` (6.3). We build
from the committed source of the fork. Keep the notices of RetroArch and
RmlUi.

## Source boundaries

The menu is in `vendor/retroarch/menu/drivers/rmlui/`:

- `driver.c` adapts the driver callbacks of RetroArch, and `menu_api.h` is
  its C boundary. In `menu.cpp` we compose the features and keep the order of
  dispatch and frames.
- `host.c` and `host.h` contain all access to the live state of RetroArch and
  its runtime operations. `files` contains the file writes and path
  operations, `declarations` the reading of the existing bounded config
  formats, and `bind_lines.h` the pure resolution of bindings.
- `Document` contains the RmlUi context, loading, rendering and element
  operations. In `View` we compose it with the presentation classes and
  attach the listeners. Each feature gets only the components it uses. The
  retained presentation lives long enough to keep the existing caches when a
  document is shut down and created again.
- `Focus` contains the focus state, and `Navigation` the rules for each
  screen. `Screens`, `Lists`, `Parts`, `Status`, `ControlView` and `Slots`
  contain their presentation state. `Controls` contains the timing of
  capture, the picker and the popup, and `binds_popup` the placement of the
  popup. `slot_tasks` contains the save and load requests of `Slots`,
  including the matching of completion callbacks.
- `Shaders`, `Discs`, `Toggles`, `Volume`, `Overlays` and `Sounds` contain
  their own operations. The built-in words are in `words.hpp`, and the
  document contract and the count of slots in `document_contract.inc`.
- `Event` contains its own payload. `Script` and `script_report` are for
  driving tests and observing them, and no production feature depends on
  them.
- `render/` contains the GL implementations, with the macOS headers kept
  apart in `platform.h`. We build the player for macOS and Windows, and not
  yet for Linux.

`rmlui_bridge.cpp` no longer exists. `rmlui_bridge.h` contains only the small
public C interface for the existing callers in the runloop, the menu and the
save tasks.

Designs are styles with optional screen overrides, and Native is the base.
We compose them with the shared Rust resolver for previews and exports. See
`screen-contract.md` for the component contract and a minimal example of an
extension.

## Build and stage

Inside an isolated worktree, first select its environment:

```sh
eval "$(uv run python scripts/worktree.py env)"
```

Commit native changes before building. On macOS you need Git, CMake, Ninja,
pkg-config and Xcode's tools, and on Windows MSYS2's UCRT64 toolchain
(see `scripts/toolchain.py` for where we look for it). Give an absolute build
folder, either a new one or one from an earlier build:

```sh
uv run python scripts/build_player.py /absolute/build/folder                            # this machine's target
uv run python scripts/build_player.py --target macos-universal /absolute/build/folder   # the macOS kit's player
```

What we build for each target is in `scripts/native_runtime/player-recipe.json`.
We refuse a fork with uncommitted changes, a folder in use by another build,
and a folder with the build of another target or anything that is not a
build. We check out the exact fork commit into the folder (with the git data
beside it, in `fork.git`), build FreeType from its pinned release and RmlUi,
both static, build RetroArch with its own mbedTLS, and record both commits and
the target in `build-info.json`, which we write last, so a folder without it
contains no finished build. Building a folder again compiles only what
changed: git rewrites only the fork's files that differ, `configure.mk` runs
configure again only when its scripts or flags change, configure keeps
unchanged outputs as they were, and every object depends on the build's own
`Makefile.local`, which contains its switches. We refuse a player, launcher or
preview renderer that links a library its system does not have (on macOS,
anything outside `/System/Library` and `/usr/lib`), and a player without the
achievements client or TLS. On macOS we build every slice for one declared
system version (11.0). For `macos-universal` we build the arm64 and x86_64
players, each as its own target in a folder named after it (x86_64
cross-compiled on Apple Silicon), join them with `lipo` into
`retroarch/retroarch`, and check that the joined file contains both. We build
the preview renderer only for the processor of the builder. The build
launches no applications and cleans none up. Check that record before using
the binary. Keep more than 20 GB free before starting another build.

To make the runtime kit of this checkout from that exact build, and the
builder from the kit:

```sh
uv run python scripts/build_kit.py /absolute/new/build
uv run python scripts/build_builder.py
```

A kit can be built only on its own platform: `build_player.py` and
`build_kit.py` on this machine make only this platform's kit. To make games for
this platform, a builder on the other platform needs a copy of it, made from
the same fork commit as its own player. The README's [Games for the other
platform](../README.md#games-for-the-other-platform) explains where a builder
looks for it, and how to pack a kit for download.

The kit records the fork commit and the hash of the player. The macOS kit is
made from a `macos-universal` build: its player contains an arm64 and an
x86_64 slice (shown by `lipo -archs`) and links only the libraries of the
system, so its `Frameworks` folder is empty and `runtime-dependencies.json`
lists no file. Use a kit of your own checkout, and never stage through a
symlink into another checkout.

The macOS kit also contains the launcher of the game,
`bin/librominabox-launch.dylib` (the `launchLibrary` of the recipe). In
`build_kit.py` we build it from `desktop/src-tauri/launcher` for both slices
and attach it to each slice of the kit's player with
`scripts/native_runtime/inject_dylib.c`, so the player loads it before its
main function. Neither depends on a game. The kit contains a digest of the
sources of the library, and in the `staging` scope we report when the
launcher in the tree has changed since then, in which case you make the kit
again. Do the same for the kit of a test with
`uv run python scripts/build_launcher.py --kit KIT`.

An export runs no compiler and no Apple tool, so any builder, including one
on Windows, can make Mac games. We copy the kit's player and launch library,
thinned for a game that runs only on Apple silicon, and read, join, relocate
and sign the Mach-O files ourselves (`desktop/crates/rominabox-engine/src/mach_o`):
each one ad hoc, and the player last, with the sandbox entitlements of the
game, sealing the app as `codesign` does. In its tests we compare each step
with `lipo`, `otool`, `install_name_tool` and `codesign` on a Mac. Where files
have no Unix modes (Windows), we write a Mac game into `<title>.zip`, which
records them (`packaging::archive`), and Archive Utility and `ditto -x -k`
restore them. The `zip` field of a request sets this on any machine.

In the builder script we find the Cargo output through `scripts/built.py`,
including `CARGO_TARGET_DIR`, then build and sign the local `.app` ad hoc. We
verify the signature and do not launch the app. This is developer signing,
not notarization. We keep the build folders for inspection.

We build the player, with its RmlUi and FreeType, for macOS 11.0, the
`deploymentTarget` of the recipe, and the launch library of the kit as well.
In the `Info.plist` of an exported app we require the newest system that any
of its programs requires, read from their load commands (a downloaded core
has its own).

## Automated checks

```sh
uv run python scripts/prepare_rmlui.py
uv run python scripts/test.py bridge padbinds shaderstate achievement-client account-input
cargo test --manifest-path desktop/crates/rominabox-engine/Cargo.toml --test design_contract --test design_composition
uv run python scripts/test.py workflows
```

In the headless suite we compile the actual document and feature modules,
with the real layout, styles, pointer events and listeners of RmlUi. In its
orchestration tests we compile `Menu`, replace only the host boundary, and
check the lifecycle, navigation, capture, failed and repeated saves and
loads, and stored settings. `menu_test_view.hpp` contains inspection for tests
only. These compiled tests replaced the earlier lifecycle and focus tests,
which were extracted from the source.

In the `workflows` scope we replay every case in
`scripts/fixtures/menu-workflows.json` without a window. The fork's own script
driver runs each script in the real menu C++ with a fake host, on menus staged
by the export's own step (`packaging::stage_menu`), and we compare every
checkpoint and written file with the baselines recorded by the launched
player. The `headless` section of the table lists the facts that only
RetroArch has, which we compare only as present or absent. In the
`workflows-native` scope we launch the few cases marked `launched` in the
exported player and keep their pictures.

Every scope that runs the player uses the test player of this checkout, which
we build once per run, before any scope starts, into `work/test-player` from
the committed fork, with the script driver of the menu and the loopback
achievements host. We keep the folder and build it again in place, so a build
compiles only what changed in the fork, which takes a few seconds when
nothing did. `ROMINABOX_TEST_BUILD` can name another build instead, which we
refuse unless it was built from the current fork commit.
Do not use `--record` to accept a difference you cannot explain. Open the
affected images and examine each failure. Timing is reported, and does not
pass or fail a test.

At a coherent final checkpoint, run `uv run python scripts/test.py --all` and
the formatting and build checks of the builder. With `--list` you see the boundaries
of each scope. The `quit` scope still leaves out one case, which requires a
bootable Dreamcast fixture that we do not have. Native tests make no sound and
exit by themselves, and after a timeout we leave the process for inspection
instead of sending it a signal. These tests are no evidence about the
placement of windows.

## Achievement workflow

With `uv run python scripts/test.py achievement-client account-input` we check
the real rcheevos client and the input protocol of RmlUi and the system,
without opening a player. In the slow `achievement-native` scope we run the
test player, built with `ROMINABOX_ACHIEVEMENTS_TEST_BUILD=1`. We export an
original cartridge and run it against a synthetic service on loopback only,
and check the evaluation of core memory, the submission of Casual awards, the
restoration of the autosave, the absence of a duplicate award after
reopening, and the behaviour when achievements are OFF or excluded. In its
state inspector we read the compressed RZIP/RASTATE format of RetroArch. Test
builds can never be frozen into the distributable kit. A live service account
is still a separate check by hand.

## Player storage and manual checks

`ROMINABOX_RML_ASSETS` selects the staged assets. Changes at run time stay in
the explicit game storage selected by `ROMINABOX_DATA_DIR`, never in another
RetroArch profile. `controls-defaults.cfg` contains the author's defaults,
and `controls.cfg` the player's overrides. In the launcher we pass one
append-config argument with the defaults before the overrides, and refuse
paths that contain a pipe. The refactor kept the file names, contents, reload
boundaries and failure policies.

Audio is off during automation. A person still has to check the actual
playback of cues, real capture from the keyboard and controllers, native
focus, pause and resume, and fullscreen transitions. The screen listeners
belong to the document, and in the headless regression we check the buttons
after the document is created again. The account tests also cover editing in
RmlUi and the AppKit composition protocol, but not physical dead-key layouts,
the placement of IME candidates or the behaviour of hardware controllers,
which we check by hand.

By default, exports have the restricted native menu: About, Hide, Quit,
Minimize and Full Screen. The author-only `advancedEmulatorAccess` turns on
the stock emulator menus, and the GUI, the command line and saved projects
share it. This restriction is part of the product interface, and separate
from the sandbox of the exported macOS app.
