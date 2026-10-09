# In-game designs and screens

A design sets the appearance of the menu components and may override
screens. Native is the base design. A screen that a design leaves out keeps
Native's declaration and markup, and a design with no `screens` array still
has the base screens. Disc uses this for Achievements. Palettes replace
colour tokens. They do not select layouts or implementations.

We compose the menu in the shared Rust engine, in
`desktop/crates/rominabox-engine/src/menu/` (`compose_menu`), for the
builder's previews, the command line and exports. The native player gets the
composed document and the existing configuration formats, and has no
inheritance of its own.

## Composition

`integrations/designs/native/menu.rml` is the document shell. A design can
provide `screen-pause.rml`, `screen-controls.rml`, `screen-options.rml`,
`save-slots.rml`, `footer.rml` and `spine.rml`, and for a fragment it leaves
out we use Native's. The heading (`#heading`) and the footer hint
(`#footer-hint`) show the `heading` and `footer` of the open screen. When we
compose the menu we write Pause's there, as the player does when the menu
opens on Pause, so the composed document is the menu as the game opens it.
The stylesheet, metrics, fonts, component templates and palette of the
selected design style the resulting document. We export only the required
base and shared assets and the assets of the selected design.

Screen declarations in `design.json` are merged by `id`. A field that a
design leaves out keeps the base value, a field it gives replaces it, and
`null` removes an optional field such as `option`. We keep the base order
unless `screenOrder` gives one, and the ids it leaves out then follow. The
navigation order depends on this. A design can place more static panels with
`<!--SCREEN:id-->` entries in `screen-order.rml` and a matching
`screen-id.rml`. Generated list panels are different: they go into the shared
`<!--SCREENS-->` insertion point.

For example, a design that only wants a different heading for Controls
declares:

```json
"screens": [
  { "id": "controls", "heading": "PAD" }
]
```

It keeps the panel, button, behaviour and markup of Controls, without a copy
of Controls or any native code. To change its arrangement too, provide
`screen-controls.rml` and keep the component ids and classes. The same rule
holds for a design with no screen overrides at all.

## Screen declarations and input

Each resolved screen has an `id`, `panel`, `heading`, `footer` and opening
`button`. A `button` field can list several element ids separated by spaces.
`option` gives its Options label and whether it is included by default, and
the author still chooses which optional entries to export. With `at: "pause"`
in `option`, the design places the button on its pause screen (RESTART), and
we list the entry in Options before ABOUT only in a design that places no
such button. For a generated
list screen with rows, we generate the back button from the declaration.
Static screens such as Pause and Controls get their markup from the composed
fragments.

**Roles.** The screens with special handling in the player have a role:
Pause, Options, Controls, Shaders, Achievements, the disc list, the
accounts of QUICK SIGN IN, HOTKEYS, the screen where we ask before we
forget the game (UNINSTALL or RESET), the screen where we ask before
RESTART, and DATA, where the player exports the game's data to a zip and
imports one after we ask. Only Native assigns roles, and a design takes one
over by replacing the screen with that id. The roles and the word for each
are declared once, as `RIB_ROLE` in `document_contract.inc`, and we read that
file in both the player and the exporter. In the player we find a screen by
its role, never by its id or its place in the declared order, so the menu
opens on the screen with the Pause role whatever is first in the
`screenOrder` of a design.

At export we write the `design.cfg` contract with one writer
(`menu/declarations.rs`):

```ini
screens = "pause options controls achievements"
screen_panel_achievements = "achievements-panel"
screen_heading_achievements = "ACHIEVEMENTS"
screen_footer_achievements = "ESC  BACK"
screen_button_achievements = "achievements"
screen_role_achievements = "achievements"
fonts = "Silkscreen-Regular.ttf"
word_slot = "BLOCK {slot}"
```

In the player we read it once per document load, into whole values. In
`Screens` we show the declared panel, hide the others, and set the heading
and footer. Every change of screen, including the opening on Pause and every
BACK, goes through one path in `Navigation`, where we then give the screen
its focus. An element listener sends an `Event` with its id, value and slot,
and in `Menu` we pass it to the code of its feature. A new behaviour needs an
implementation and tests in that feature, and a layout override needs
neither.

**Navigation** is RmlUi's: an arrow moves the focus to the element next to
it on screen. The elements that can take the focus are declared once, in the
shared `parts/navigation.rcss`. In its own stylesheet a design can override a
single move with `nav-up|right|down|left`, and choose where any screen
starts with `autofocus`, including Pause and the pad screen. The focus stops
at the edges.

**Fonts** are the files listed under `fonts` in `design.json`. At export we
stage them beside the document and list them in `design.cfg`, and in the
player we draw with them.

**The canvas.** Every design is laid out on one reference canvas, 960 × 600
dp, which we scale in the player to fit the whole window. It is `RIB_CANVAS`
in `document_contract.inc`, and a stylesheet can write
`design(canvas-width)dp` and `design(canvas-height)dp`.

**The game's shape.** Each time the menu opens, we set the shape of the
running game on the document as `data-game-shape`, its width over its height
to three decimals. Every element that a design marks `data-game-shaped` gets
that shape, inside the `max-width` and `max-height` declared in the
stylesheet of the design: we set the width and height of the element, in dp,
and nothing else, and leave alone an element without such lengths. The
player code names no element for this, and the design chooses what takes the
shape, how large it is and where it goes. Native and Disc mark the
`.slot-picture` of each slot, at most 230 × 138 dp, and the saved picture
fills it, so it never has bars. Both attributes are declared in
`document_contract.inc`.

The native contract of ids and classes is
`vendor/retroarch/menu/drivers/rmlui/document_contract.inc`. We use it in the
C++ and in the exporter (`menu/contract.rs`), where we check every composed
menu against it. It is grouped by role, so "required" means required whenever
that screen ships. The count of six slots is defined there once. The
generated ids and suffixes, and the state classes and attributes that we set
in the player, are declared there too, and the player code refers to no
element in any other way.

## Words

In the player we write some words ourselves: the statuses of the pad screen,
the footer while a binding is captured, the words of the slots and the
messages of saving and loading, the count of a pager, the labels of the disc
list, the mark on the current row of a list (the running filter, the disc in
the tray), the states and buttons of the achievements screen, the keys of the
on-screen keyboard, and the names of the player settings, the ends of a level
and the state of a switch, which we write into the document when we compose
it and keep current in the player. They are declared once, with the current
English, in `vendor/retroarch/menu/drivers/rmlui/words.inc`. A design gives
its own wording in `design.json`:

```json
"words": { "slot": "BLOCK {slot}", "empty": "FREE" }
```

`{slot}` and the others are values that we fill in in the player, and the
words of a design may use the values of the English wording. We refuse at
export an id that the player does not write, or a value that we do not fill
in there. A word that a design does not give stays in English. The heading
and footer of a screen are not words: they come from its `screens` entry.
Neither is the prompt of a status line: it is the text of the markup, which
we keep in the `data-prompt` of the line and show whenever no status covers
it.

## Lists and common parts

All lists share one implementation of rows, paging and focus. `row.rml` can
replace the built-in row, and must keep `ROW-ID`, `ICON`, `TITLE`, `DETAIL`,
`STATE`, `SELECTED`, the class `list-row` and the state id `ROW-ID-state`.
The palette and the component metrics give the list its styling, so avoid
copies of that styling for one feature. Native's `list.pageSize` is the
default that a design inherits. The pager ids are `{screen}-prev`,
`{screen}-next` and `{screen}-page-count`.

At export we generate the rows and the screen links from the bundled data. A
screen that Native declares with `"rows": "licences"` (ABOUT) is a list of
the components in the game, each with its licence, from the licence index of
the game. Its status line gives the location of the licence texts. It is a
screen with no role, and in every design it is the last Options entry before
UNINSTALL or RESET. In the player, paging and the presentation of rows are in
Native `Lists`, and the runtime operations of `Shaders`, `Discs` and the
accounts of QUICK SIGN IN are each in the code for the screen of that role.

A design may provide `parts/slider.rml`, or it gets the shared slider. The
holes of its template are `PART-ID` and `LABEL`. With `parts/toggle.rml` a design
draws a player setting that it places itself. The component classes include
`slider`, `slider-track`, `slider-fill`, `slider-thumb`, `slider-readout`,
`toggle`, `toggle-knob` and `toggle-label`.

## Player settings

The settings that a player changes in the game's own Options are declared
once, in `desktop/crates/rominabox-engine/src/player_settings.rs`: an id, the
word for its name, a kind (a level or a switch), the RetroArch key it sets
and the default of the export. The keys a setting can set and the kinds that
show one are declared once in the player, in
`vendor/retroarch/menu/drivers/rmlui/settings.inc`, and we read that file in
the exporter, the menu and the host. A key that the host cannot apply stops
the build of the player, and a key missing from the list stops the export.
Today the settings are the volume (`audio_volume`), PLAY IN BACKGROUND
(`pause_nonactive`, where false means on) and RUMBLE (`input_rumble_enable`,
the fork's own switch: when it is off, every joypad driver gets no rumble).
When we compose the menu, we draw every setting in the Options of every
design, without the design naming it:

- A level is the slider `<id>-level` with the arrows `<id>-down` and
  `<id>-up`, the ends `<id>-low` and `<id>-high` and its name, in
  `#<id>-control` at the start of the Options panel. The ids of the volume
  are `volume-level` and so on, and it keeps the classes `volume-arrow`,
  `volume-end` and `volume-name`.
- A switch is one more Options entry, drawn with the entry template of the
  design after the screens and marked `switch`: its label, then `<id>-state`
  with the word for its state. At run time we keep the word current and set
  the fact `on`.
- For a setting in another place, a design writes `<!--SETTING:<id>-->`
  there, and we put a level's slider or the design's toggle part for a switch
  in its place.

A setting that the running game has no use for is `disabled`. For each key
the host gives the answer (`rib_host_used_<name>`), and rumble is of use only
when the core asked RetroArch for the rumble interface. A core can ask at
start, at load or on its first frame, so we check it each time we paint the
settings, and nothing about it is decided at export. The shared `toggle.rcss`
hides a disabled switch, and `navigation.rcss` already keeps the focus away
from it.

In `design.cfg` we give the player the control, key and file of each drawn
setting, and the value of a level at each position, low end first
(`setting_values_volume = "-80.0 -38.2 … 0.0"`). Only in the player do we
find the position for a value, and a value not in the list goes to the
nearest one. The values of the volume are declared once, in
`vendor/retroarch/audio/volume_range.h`: silence, then steps that sound even
up to normal. Its words are the player's (see [Words](#words)). In
`PlayerSettings` we apply a change at once through the host, and write
`key = "value"` to the setting's own file in the game's data (`volume.cfg`,
`background-play.cfg`, `rumble.cfg`). The launch plan lists every setting
with its default as `player_setting<TAB>file<TAB>key<TAB>default`. In the
launcher we apply the player's file when there is one and the default when
there is not, and never write the default into the player's file.

Each step of a change of level makes one sound, at the new level: the
movement cue of the pack, or, in a game with menu sounds Off, the volume's
own tick (`integrations/parts/volume-tick.wav`), which we ship only in that
case. The bottom step is silence, and there we play no cue.

## Overlays

Overlays are drawn while the game runs, and the game keeps its input. A
design declares when each one appears, stays and leaves, for example:

```json
"overlays": [
  { "id": "splash", "afterMs": 0, "holdMs": 550, "leaveMs": 250,
    "needs": "splash-logo.png" },
  { "id": "notice", "follows": "splash", "afterMs": 700,
    "holdMs": 3600, "leaveMs": 500 }
]
```

The element gets `showing`, then `leaving`, and the body gets `overlay` while
the menu is closed. In RCSS, `design(overlay-leave-<id>)` is in seconds, so
the style and the timing come from one declaration. An overlay runs only when
its element exists and its optional `needs` asset was shipped. The timeline
is in `Overlays`, and `rib_rmlui_begin_overlays()` is still the entry point
when the core starts.

## Verifying changes

For composition and exported assets, use the Rust integration tests
`design_contract` and `design_composition`, and `menu_snapshot` for the
composed files byte for byte. In the `navigation` scope we compose every
registered design and the hypothetical ones under
`desktop/crates/rominabox-engine/tests/fixtures/designs/`, run the real menu
C++ without a window against the case tables in
`scripts/fixtures/navigation/`, and measure the picture of a save slot for a
4:3, a 16:9 and a 10:9 game. In the `bridge` and `workflows` scopes we do
the same for the actions of the menu and the recorded workflows. Look at the
affected pictures yourself as well as reading the assertions. See
`native-runtime.md` for the commands and for which checks are automated and
which are done by hand.

## Account controls

When achievements are included, we compose `screen-achievements.rml` from
Native unless the selected design supplies its own. The base
`achievements.rcss` uses the metrics and palette of the selected design, and
an optional `achievements.rcss` of the design adds overrides after it. To
change the style of a field, button or dialog, you don't need a copy of the
screen. The optional `pageSize` of a screen overrides the list capacity of
the design where the account controls take up vertical space.

The identities of the controls are in the native `document_contract.inc`,
and a replacement account screen must keep those controls and the shared list
insertion points. Text editing is RmlUi's. A small platform adapter provides
the composition of characters and the clipboard of the system. Signing in is
supported with a physical keyboard. The on-screen keyboard stays as it is,
with no further development. The keyboard and pads use logical controls, the
volume arrows work only with the pointer, and Tab moves only through the
fields and buttons of the sign-in form.
