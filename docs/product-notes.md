# ROM-in-a-Box: product principles

What the builder and the games made with it are, and what they are not.

## Authoring

- The first screen offers only dropping or choosing a game. Then come the lookup progress, the details, filled in automatically and editable, the menu and the export. Settings never come before the game.
- The happy path is: drop a game, look it up, adjust if wanted, export. A supported game with no catalog match can still be packaged, with an editable name and console.
- Familiar controls and short labels. Optional help icons give more detail, also on keyboard focus. Advanced settings are available without crowding the default flow.
- Lookup and artwork are on by default, and can be turned off and edited. The game itself is never uploaded.
- Using the builder never requires installing programming runtimes, toolchains, shells or helper programs, or elevated rights. When a platform capability is missing, the export fails with a clear message before it starts.
- Everything in the builder is also in the command line, through the same engine and project format, with structured output and an agent skill.
- A saved project (`.rominabox`) contains the game, its pictures and the settings, so it opens again anywhere without repair. It contains no emulator runtime, credentials or player saves.

## The game

- An export is one self-contained app per platform. It contains the chosen menu design and its assets, the shared player files, the chosen core and the licence texts, and nothing of the builder, other designs, lookup catalogs or test tools.
- A game starts straight into play, quickly. RetroArch's own startup screens and routine notifications are hidden. An optional, short ROM-in-a-Box splash may play, without adding any waiting.
- **Esc** opens the menu during play: Continue, six save slots with pictures, Save, Load, Options, Restart and Quit. Before we restart the game, we ask the player. The author can make the menu open at startup instead, and can leave Restart out.
- Options contains the player's settings in the same designed menu: volume, controls, hotkeys, shader filters, achievements, a disc list for multi-disc games, play in background, rumble, ABOUT and, last, UNINSTALL (Windows) or RESET (macOS).
- Windowed and fullscreen play both work, and Alt+Enter switches between them. The menu works in fullscreen and with a controller. A window opens on an available display, with legible text and correct hit areas, whatever the size or pixel density of the display.
- The person who bundles a game chooses its BIOS, and the game offers its player no BIOS choice. Someone who wants a different BIOS turns on the advanced emulator access, which opens RetroArch's full menus.
- Each game keeps its configuration, saves, states, memory cards, core options, overrides, remaps, screenshots, caches, logs and credentials in its own storage, named after the identity of the game. It never falls back to another RetroArch installation or to global settings. Renaming or moving the app keeps its saves.

## Controls

- The identity of a control (what the emulator receives), its label (what the player reads) and its binding (the key or pad input) are separate. An author can label A as Jump without changing the signal. The original control stays visible as context.
- In the game, Controls is a drawn controller with callouts attached to its buttons. Each callout shows the input and, when one is set, the label of the action. The keyboard, a pad and the pointer reach the same operations. Consoles without artwork get a compact list.
- In the builder, the controls are compact rows under Advanced, from the same declarations. The author writes the action labels there, and the Controls screen of the game changes only the bindings.
- A binding capture takes the first input, with a visible countdown and Cancel. After a timeout, a Cancel or a disconnected pad, the old binding stays. The input that started the capture is ignored until it is released.
- We record a pad input as a position on the standard pad, never as the button number of one model, so a binding works on any pad.
- Every connected pad plays as player 1, unless the author turns that off for a game with a second player.
- Reset restores the defaults shipped with the game. The player's changes stay in the game's storage.
- Controller variants are part of the description of the console: the Mega Drive has three- and six-button pads, and the drawing, callouts, bindings and emulated device always agree.
- The controller artwork is original or licensed for redistribution, and we keep its provenance.

## Shader filters

- The author picks a small set of shader filters to bundle, and the one to start with. The player switches between them, or to no filter, under Options.
- The detailed configuration of shaders stays behind the advanced emulator access.

## Achievements

- RetroAchievements support is included by default and can be left out. With it we package the integration and its screens, never downloaded achievement data or an author's account.
- The player signs in under Options, Achievements. We keep the sign-in and the on/off choice only in that game's storage. Achievements use the normal Casual mode: no Hardcore, no save lock, and pause and saves work as usual.

## Look

- Menu designs and colour palettes are separate choices. A design sets the typography, shapes, borders, icons, arrangement and interaction, and a palette changes only colours.
- The menu looks made for a cartridge-era console: deliberate bitmap lettering, a consistent pixel grid, stepped shapes, bold saturated colours with strong contrast, constructed borders with light and dark edges, and crisp offset or inset shadows, never blur. The selection is obvious with the keyboard and with a pad.
- The builder's preview uses the game's own renderer, so a preview is what the game shows.
- Menu sounds are optional: one complete pack of move, confirm and back cues, Off by default.

## Size and speed

- A small cartridge game is below 26 MB on disk. We download the core of a game when an export needs it, and never bundle cores with the builder.
- Report the size of an app on disk, with the runtime apart from the game's content. Removing unused modules is not by itself a size improvement.
- Measure the cold launch, the time to the first responsive frame, and pausing and resuming, separately.
- Interface work does not wait for emulator startup or packaging: previews and fixtures reach the states of the menu directly. Headless tests do not establish the native appearance, focus or launch speed.

## Systems

- Recognition, metadata lookup, core selection and packaging are replaceable parts. Configured support for a console does not mean that every combination has been played.
- Planned consoles include Game Boy, Game Boy Color, Game Boy Advance, NES, SNES, Mega Drive, Master System, Game Gear, Nintendo 64, PlayStation, Dreamcast, GameCube and PlayStation 2, each as far as its core allows.

## Later

- Linux games and builder.
- Importing custom menu designs as packages of RML, RCSS and assets.
- Android games, as another player target. Layouts keep the state of the menu independent of landscape geometry, so they can reflow for portrait screens and touch.
