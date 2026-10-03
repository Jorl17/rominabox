# Engineering expectations

Build in small, working slices. Keep the GUI and the command line on the same application services and project format. Separate metadata providers, runtime control, storage and platform packaging where their responsibilities differ. Introduce abstractions for real variation, not speculative frameworks.

## Declarative design

Prefer declarations that turn on complete, reusable behaviour. Handle runtime capabilities, packaging rules for each target and preset definitions in one central place, not separately in GUI screens, command line commands and build steps. Extend a framework through its intended means of composition before replacing its lifecycle with repeated custom wiring.

A mixin or a registration can itself be the interface, when adopting it brings a coherent capability. What matters is a clear contract, an implementation that is easy to find, composition that is easy to follow, and tests for the supported scenarios and their interactions. Avoid opaque inheritance chains, and configuration languages that only restate procedural code. A caller should state its intent, without rebuilding the machinery behind it.

Keep the invariants of a domain operation in that operation. Put external interactions behind configurable boundaries, with a clear place for each side effect. Call sites should not have to repeat the orchestration.

## Tests

Outside-in workflow and integration tests are the foundation, and they must exist. Unit tests are strongly welcome, for precise coverage and fast feedback alongside real workflow tests. There is no fixed ratio between the two, and neither replaces the other.

- Integration tests cover the generated configuration, runtime commands and their completion or failure, the pairing of save states and previews, the persistence of memory cards, and the preservation of player data when an app is renamed or rebuilt.
- Run the same authoring scenarios through the GUI and the command line, so their behaviour cannot diverge. Test actionable errors and structured command line output as well as successful builds.
- Keep our own components real in workflow tests. Mock external boundaries where needed, and also test the actual emulator integration in dedicated runtime tests.
- Unit tests cover meaningful rules: manifest validation, the confidence of identification, path resolution, argument handling and dependency selection. Avoid tests that only repeat the implementation.
- Test declarative capabilities and their supported combinations through the shared implementation, including conflicts, invalid declarations and interactions. A happy-path test of each declaration on its own is not enough when declarations combine.
- Test packaged apps on Windows and macOS with redistributable homebrew or test programs. Check a launch without a separately installed emulator, offline use, paths with spaces and Unicode, fullscreen and menu transitions, controller use, and saving and loading after the app restarts.
- Keep commercial games and firmware out of CI and Git. Real Mega Drive and Game Boy Color games, played by hand, add to the automated tests. Record the configured support apart from the combinations actually tested.
- Verify checksums, required notices and matching metadata for access to the source as part of release packaging. Check the contents of unsigned packages apart from signatures and timestamps, which can vary between builds.

Set up formatting, linting, type checking where supported, and the relevant tests with the first slice of an implementation. Pin the settings and other inputs that change what a test exercises. Never weaken an assertion to hide a failure. Do not claim that a test was run when it was only planned, and report gaps in platforms or hardware explicitly. Repeat a successful check only when changes or new evidence justify it.

### Red-green

Every bug fix, including a mistake found during feature development, needs a regression test that shows the faulty behaviour. Run the test before the fix and confirm that it fails for the intended reason, apply the fix, and run it again to confirm that it passes. The test should cover the blind spot that the bug exposed, not only the exact lines changed. If the fix already exists, show the test against the earlier faulty implementation on its own. Keep unrelated work, and say so when a meaningful reproduction cannot be run.

New features do not need test-first development. It is fine to implement a feature and then write its tests, and speculative suites of failing tests are not required. Once iteration uncovers a mistake, use red-green to establish lasting coverage.

Use targeted red-green checks for important non-functional properties too. For example, a cache test can first show repeated network requests without the cache, then pass once the intended reuse exists. Prefer stable counts or resource bounds over fragile wall-clock thresholds. A performance test must be shown to catch the inefficient behaviour it is meant to prevent.

## Development workflow

Provide reproducible commands for setup, build, tests and cleanup, with useful help and without interaction where possible. Isolated checkouts must not compete for mutable state. Look at the existing conventions before changing them.

Use guard clauses, meaningful types and validated inputs at boundaries. Document public and non-trivial function contracts, and keep code compact without fighting the formatter. A long-running build needs visible progress, failure, cancellation and recovery, with useful diagnostics available apart from concise errors for the user.

## Documentation

Keep the README short and usable. Document the real setup steps, runnable examples, the command line and the project format, the supported and tested combinations, and the known limitations. Update the affected documentation when behaviour changes.

Explain an architectural decision when its trade-off matters. Use plain language and concrete examples. Avoid promotional claims, repeated summaries, redundant help text and pages that restate obvious code behaviour. Keep proposals apart from implemented features.

## Feedback-loop speed

Test coverage and the speed of feedback are both engineering requirements. During iteration, run the regression or affected suite that answers the current question. Broaden to the relevant integration workflows when a coherent change is ready, and run the full checks at meaningful checkpoints and before merging. Do not rerun successful, unaffected suites again and again without new evidence.

Measure how long tests and checks take. Investigate unexpected slowdowns promptly: unnecessary process startup, network access, shared setup and redundant work are defects in the development workflow. Keep routine tests isolated and headless, cache immutable build inputs, and keep desktop checks for the behaviour that actually requires a desktop. Improve execution and test selection without dropping important assertions, and without replacing integration coverage with mocks of our own components.

The tests are grouped into scopes. Run `uv run python scripts/test.py --list` for a description of each scope and what it deliberately does not prove, `uv run python scripts/test.py <scope>...` for some of them, the same without scopes for the fast ones, and with `--all` for the slow ones too. We compare the time of each scope with the last measured time (`scripts/fixtures/scope-budgets.json`), and at the end of every run we report anything left in the per-user folders or the temporary folder. The timing reports are diagnostics, not wall-clock assertions.

## Menu designs

A design is a package under `integrations/designs/`. Its `design.json` declares the screens, roles, words, palettes and fonts, and its RML and RCSS draw them. In the exporter (`desktop/crates/rominabox-engine/src/menu/`) we compose the one design an author picks, with the shared parts in `integrations/parts/`, into the menu of a game. In the menu runtime in the fork (`vendor/retroarch/menu/drivers/rmlui/`) we set facts such as `focused`, `selected` and `disabled`, and the RCSS of the design sets their look. The contract between the two is [screen-contract.md](screen-contract.md), and [design-authoring.md](design-authoring.md) is the guide to writing a design.

The builder's preview (`rml-preview`) draws a composed menu with the player's own renderer and with the code of the menu runtime for reading a design, showing a screen and splitting lists into pages, so the picture of a screen is that screen as a game shows it.

## Export contents

At export we copy the player and its launcher from the runtime kit of the target platform, compose the menu from the selected design, palette, sound pack and filters, and add the core we downloaded for the console. Nothing of the builder goes into a game, and nothing of a design the author did not pick. In the `shipped` scope we check the contents of a game and how its launcher reads them, and in the `size` scope we keep a small cartridge game below 26 MB.

With `showMenu` the author sets whether a game has its menu at all. A game with a menu can open it during play.

## The builder's Rust code

The builder's Rust code is one Cargo workspace, `desktop/Cargo.toml`, with two packages. `desktop/crates/rominabox-engine` contains everything that the builder and the command line do, and `rominabox-cli` itself. `desktop/src-tauri` is the package of the window: Tauri, its configuration and `main.rs`, which calls the engine. The bundled resources and the C sources of the launcher, the preview renderer and the accounts store are beside it, and Cargo does not build those. Tauri's build script runs again whenever a file bundled with the builder changes, such as the runtime kit or the command line copied into `resources/bin`, and the package it belongs to is compiled again, which in its own package means only `main.rs`. With `scripts/built.py` we build the command line with features resolved over the whole workspace, as in the build of the builder, so both use one compile of the engine.
