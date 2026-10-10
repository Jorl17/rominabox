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
- Windowed and fullscreen play both work. With Alt+Enter, or Option+Return on a Mac, the player switches between them, during play and in the menu, and also with FULLSCREEN, a hotkey the player can bind on HOTKEYS. On a Mac we use the system's full screen, which the player also reaches with the window's green button. The menu works in fullscreen and with a controller. A window opens on an available display, with legible text and correct hit areas, whatever the size or pixel density of the display.
- The person who bundles a game chooses its BIOS, and the game offers its player no BIOS choice. Someone who wants a different BIOS turns on the advanced emulator access, which opens RetroArch's full menus.
- Each game keeps its configuration, saves, states, memory cards, core options, overrides, remaps, screenshots, caches, logs and credentials in its own storage, named after the identity of the game. It never falls back to another RetroArch installation or to global settings. Renaming or moving the app keeps its saves.

## Controls

- The identity of a control (what the emulator receives), its label (what the player reads) and its binding (the key or pad input) are separate. An author can label A as Jump without changing the signal. The original control stays visible as context.
- In the game, Controls is a drawn controller with callouts attached to its buttons. Each callout shows the input and, when one is set, the label of the action. The keyboard, a pad and the pointer reach the same operations. Consoles without artwork get a compact list.
- In the builder, the controls are compact rows under Advanced, from the same declarations. The author writes the action labels there, and the Controls screen of the game changes only the bindings.
- In a binding capture we take the first input the player presses, with a visible countdown and Cancel. An input with no position in the profile of the pad we take when the player releases it, unless the player presses one with a position first, so a trigger that is both a button and an axis binds as the trigger. After a timeout, a Cancel or a disconnected pad, the old binding stays. We ignore the input that started the capture until the player releases it.
- The player can bind any input of a pad, on HOTKEYS and on CONTROLS, from any pad that plays as player 1. On HOTKEYS we record an input that has a position in the profile of the pad as that position on the standard pad, so the binding works on any pad, and any other input, such as a touchpad, as itself. The defaults of an export are positions only. In the game we show each input by its name in the profile of the pad in use, such as Cross, except the shoulder buttons, the triggers and the stick clicks, which have the same place on every pad and keep their standard names, such as L1. In the builder we show the standard names.
- A rebind on CONTROLS applies to every pad that plays as player 1. On a pad of another model, the same button number can be another button.
- Every connected pad plays as player 1, unless the author turns that off for a game with a second player.
- Reset restores the defaults shipped with the game. The player's changes stay in the game's storage.
- Controller variants are part of the description of the console: the Mega Drive has three- and six-button pads, and the drawing, callouts, bindings and emulated device always agree.
- By default QUICK SAVE, QUICK LOAD, PREVIOUS SLOT and NEXT SLOT are on F2, F4, F6 and F7, and on the shoulder buttons that no pad of the console uses, with save on the left and load on the right. When the pads use none of L1, R1, L2 and R2, save and load go on L1 and R1, and the slot hotkeys on L2 and R2. When they use L1 and R1 but not L2 and R2, save and load go on L2 and R2. Otherwise these hotkeys have keys only. We count every pad in the console's picker, so a default stays free when the player switches pad. When the author changes the console in the builder, we replace only the pad bindings that came from the defaults for the console before.
- PREVIOUS PAGE and NEXT PAGE turn the pages of a list in the menu: by default Page Up and Page Down, L1 and R1, and a mouse wheel tilted left or right. They turn the list with the focus, or else the first list on the screen.
- A hotkey that acts only in the menu and one that acts only during play may share an input, because they never act at the same time: L1 can be PREVIOUS PAGE in the menu and QUICK SAVE during play.
- Wherever the menu names a hotkey, in a footer, on the pause notice and in the badges on Back buttons and on the arrows of a list, it shows the player's own binding, of the input in use: a key after a key or the mouse, a pad input after a press on a pad, and before any input a pad's when one is connected. A hotkey with no binding of that kind shows one of the other kind.
- The controller artwork is original or licensed for redistribution, and we keep its provenance.

## Shader filters

- Options has VIDEO, with BRIGHTNESS and CONTRAST, each shown as a percentage, and SHADERS when the author bundles shader filters. The author can leave VIDEO out, and chooses the brightness and contrast the game starts with.
- The author picks a small set of shader filters to bundle, and the one to start with. The player switches between them, or to no filter, on VIDEO.
- Above 100 %, a bundled filter with a brightness setting of its own brightens through it first, as far as it goes, so its picture keeps its look. We measure how much light each value gives with `scripts/measure_shader_brightness.py`. A filter the author adds brightens the same way through a parameter it declares whose name or description says brightness, bright boost, luminance or gain (a plain "brightness" first), and we take its light to rise in proportion to its value. The builder only reads the filter's files and runs nothing.
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
