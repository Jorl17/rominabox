---
name: rominabox
description: Turn a retro game file into a standalone Mac or Windows game with ROM-in-a-Box's command line, rominabox-cli; look a game up, choose its menu, controls and picture filters, save or reopen a project, and export.
---

# ROM-in-a-Box

With ROM-in-a-Box you turn a console game file (a cartridge ROM, or a disc image with its tracks) into a game that opens straight into play, with a pause menu, saves and its own controls. `rominabox-cli` is its command line, and everything you can make with the ROM-in-a-Box app, you can make the same way with it.

## The tool

- On Windows, run it by name: `rominabox-cli`. It is on the path after installing ROM-in-a-Box.
- On a Mac, it is inside the app: `/Applications/ROM-in-a-Box.app/Contents/Resources/bin/rominabox-cli`. Where this page has `rominabox-cli`, run that path.

If it is not there (on Windows, if the shell cannot find it), ROM-in-a-Box is not installed. Tell the person so, and that they can install it from https://github.com/Jorl17/rominabox/releases. Do not download or install it yourself, and do not look for it in other folders.

Run `rominabox-cli --help` for the list of commands, and `rominabox-cli schemas` for a description of every request field. The output of `schemas` is the reference when this page and the tool disagree.

## Requests and output

For a command with a request, give one JSON object on stdin, up to the end of the input. The output is JSON Lines on stdout. Use absolute paths. In JSON, write a Windows path with forward slashes (`C:/Users/me/game.sfc`) or doubled backslashes.

- The output of `export` and `project-save` starts with what was found. `{"type":"identified",…}` is the lookup (name, console, cover, `warnings`), absent when the request has both `title` and `system`. `{"type":"content",…}` contains the files that go with the game. For a console that takes a BIOS, `{"type":"firmware",…}` follows. Pass on any `warnings`, the firmware `notices`, and any BIOS file that did not count.
- `{"type":"progress",…}` lines come while the work goes on.
- `{"type":"result","result":…}` comes at the end of a successful command, with exit status 0.
- `{"type":"error","message":…}` comes with exit status 1.
- `{"type":"exists","appPath":…}` comes when a game is already where this one would go. Nothing was written, and the exit status is 1.

Wait for `result`, because progress lines do not mean success. On an error, report its message, and do not delete files and retry the same thing.

## Make a game

The shortest way, the same as dropping the file on the app:

```sh
rominabox-cli export /path/to/game.sfc /path/to/folder
```

With this, the game is looked up (name, console, cover), made with the app's settings, and written into the folder, or into `ROM-in-a-Box` in Downloads when you give no folder. The game is for this computer:

- on Windows, one `.exe`, which sets itself up the first time it opens;
- on a Mac, a `.app`.

A request on stdin can set more. Whatever it leaves out comes from the app, and whatever it states wins, including `null`:

```sh
rominabox-cli export <<'JSON'
{"rom":"/path/to/game.sfc","title":"My Game","palette":"green","startAtMenu":true,"outputDir":"/path/to/folder"}
JSON
```

- Run `rominabox-cli defaults` for the settings used for what a request leaves out, and `rominabox-cli places` for where games go, the platform they are for, and the caches.
- In the output of `rominabox-cli designs` are the menu `designs` (the `theme` of a request), the colour `palettes`, any of which works with any design, and the `soundPacks` (the `menuSounds` of a request).
- A request with both `title` and `system` is not looked up. Otherwise the title is the catalogue's. Keep it unless the person gives another, because a later rename starts the game with no saves.
- With `online:false`, only earlier lookups are used, and nothing goes to the network. A lookup downloads catalogues and covers and matches the game on this computer, and nothing from the game is sent.
- A game already at the destination stays as it is, unless the request has `replace:true`.
- The saves of a game belong to its console, title and game file together. An export with those unchanged keeps the player's saves, and a new title or a changed file makes a new game with none. A new cover or new settings keep them.
- With `target` (`macos` or `windows`) you make the game for the other platform, and with `bothPlatforms:true` both, in one `<title>.zip` with `Mac/` and `Windows/`. A game for the other platform needs the runtime kit of that platform, which is downloaded the first time. When the download is not possible, the export stops with the reason. On a Mac, `intelMacs:true` makes a game that also runs on Intel Macs, about twice the size. On an Intel Mac that is the default.

## Look before exporting

- With `inspect` `{"rom":…,"online":true}` you get the `title`, `system`, cover (`iconPath`) and `warnings` for an export. `matched:false` means that no catalogue entry was found. Ask for a name instead of inventing one.
- With `content` `{"rom":…}` you get every file an export would take with the game (the tracks of a disc, for example).
- With `systems` `{}` you get the consoles, each with its core. The core is downloaded when an export needs it.

Export a disc game made of several files from its `.cue`, `.gdi` or `.m3u`, never from one track.

## Choices

- **Menu:** With `showMenu` (on by default) the game has its pause menu, and with `startAtMenu` the menu opens first. `theme`, `palette` and `menuSounds` come from `designs`. `background` is a picture behind the menu, shown as it is, and with `tintBackground` it is drawn in the colour of the palette, so the text of the menu stays readable on it. `splash` is the short logo at the start. With `includeAchievements` the player can sign in to RetroAchievements from the menu. With `keepPlayingInBackground` the game keeps running in the background, and with `autosaveOnQuit` (on by default) the game is saved when the player quits. With `everyPadIsPlayerOne` (on by default) every connected pad is player 1. Turn it off for a game with a player 2. With `advancedEmulatorAccess` the player can also open RetroArch's full menu. In every game, Options has an entry to remove what is stored for the game on the computer: UNINSTALL on Windows, RESET on a Mac.
- **Controls:** With `controls` `{"system":"megadrive"}` you get the console's controls with their ids, labels and default keys, and its `variants` (the six-button pad, the analog one). Give one in `controls.profile`, and with that `profile` in `controls` you get its controls. In an export, `controls.bindings` maps a control id to `label`, `key`, `pad` (a position on the standard pad: `up`, `down`, `left`, `right`, `a`, `b`, `x`, `y`, `l`, `r`, `l2`, `r2`, `l3`, `r3`, `select`, `start`, or a stick direction such as `l_x_plus`) and `mouse`. The A button of a console is not necessarily the `a` of the pad, so read the ids. Give a button a label for its action only when you have been told what it does.
- **Hotkeys:** With `hotkeys` you set the bindings for opening the menu (`menu`), confirming (`confirm`) and going back (`back`) in it, and, while the game plays, for saving to the selected save slot (`quick-save`), loading from it (`quick-load`) and changing it (`previous-slot`, `next-slot`). In a game with fast forward (`fastForward`), there is also `fast-forward`, with its speed in `fastForwardSpeed` and, in `fastForwardHold`, whether it works only while held. With `fullscreen` the player switches between fullscreen and a window, during play and in the menu. It has no binding by default, and the player can always do the same with Alt+Enter, or Option+Return on a Mac. Each is a list like `["key:escape","pad:home","pad:l3+r3"]`, and the four for the save slot default to F2, F4, F6 and F7, with the shoulder buttons that no pad of the console uses. With `rominabox-cli hotkey-defaults` and `{"system": ...}` you get the hotkeys a game for that console starts with, and we give them to every hotkey a request leaves out. MENU must keep a key, and CONFIRM and BACK a binding, and the others may be empty. A hotkey that acts while the game plays may not have one of the game's keys, or a pad button that the game reads (a chord such as L3+R3 is allowed). Any other set is refused at export, with the reason. Check a set first with `rominabox-cli hotkeys-check`, which takes `{"hotkeys": {...}, "system": ..., "controls": {...}}`. The player can change the hotkeys in the game. A project saved with `menuControls` still opens.
- **Video:** With `video` (on by default) the game's Options has VIDEO, where the player sets brightness and contrast. They start at `brightness` and `contrast`, multipliers from 0.5 to 2.0 and from 0.67 to 1.5, at the positions of their sliders. The player chooses the picture filters on VIDEO, so a game with filters needs `video`.
- **Picture filters:** With `rominabox-cli shaders` you get the bundled filters, for any console: ROM-in-a-Box's own and libretro's (CRT Royale and others, with their `authors`). In an export, `shaders` has the `bundled` ids, the `custom` presets (`{"path":…}`, GLSL `.glsl`/`.glslp` or slang `.slang`/`.slangp`, with an optional `"name"`, or else the file name without that extension) and the `initial` one. All the filters of a game must be in one language. CRT Guest exists only in slang, so it cannot go with a GLSL preset of the person's. Check a selection first with `rominabox-cli shaders-check`.
- **BIOS:** With `firmware` `{"system":"ps1","files":[…]}` you learn whether the files are enough (`canContinue`), with each file that did not count and the reason. Give the export the same paths in `firmware`. Use only BIOS files you were given, and never search the folders of another emulator for them. Some consoles run without one.

## Projects

A project keeps a game file, its BIOS, pictures and every choice together in one `.rominabox` file, without the emulator and without the player's saves.

- `project-save` `{"archivePath":"/path/to/game.rominabox","settings":…}`: the game as an export request describes it, without where or from what it is built (`outputDir`, `replace`, `runtimeKit`, `core`, `coreCache`). Settings with only `rom` are completed as for `export`, including `target`, the platform of this computer unless stated.
- With `project-open` `{"archivePath":…,"extractionDir":"/path/to/new/folder"}` you get the settings, with paths to the extracted files.

A project contains the game file itself. Say so before anyone shares it.

## Menu picture

With `preview` `{"outputDir":…,"theme":…,"palette":…,"background":…}` you get a picture of the menu without starting a game, at `imagePath`. Whatever it leaves out comes from the app. In the picture you see how the menu looks, not how the game plays.

## Care

- Do not open or run a game you made unless you are asked to. A finished export is not a reason to launch it.
- Keep game files, BIOS files and projects private, because they belong to the person.
- When an export finishes, report the path of the game (`appPath`) and its size once set up (`installedBytes`).
