# Writing a menu design

A design is the look of a game's in-game menu: the pause screen with its six
save slots, Options, the pad screen and the other screens. Every exported game
uses one design and one palette, and the player can use the menu with a
keyboard, a pad or a mouse. This guide is for someone making a new design. It
covers what a design can change, the files involved, and how to check the
result. [`screen-contract.md`](screen-contract.md) is the reference behind it.

The menu is [RmlUi](https://mikke89.github.io/RmlUiDoc/) markup (RML, close
to HTML) and stylesheets (RCSS, close to CSS), drawn by the game's player.
There is no JavaScript. In the player we show the screens, move the focus,
save and load, and set classes and attributes for the current state. The
design sets what everything looks like and where it goes.

## What a design is

A design is a folder beside Native, the base design:

```
integrations/designs/
  native/        the base, which every design inherits from
  disc/          a shipped design built on Native
  mydesign/      yours
```

Whatever your design leaves out comes from Native. That allows three levels
of change:

- **Restyle.** Supply your own `menu.rcss`. The screens keep Native's markup
  and ids, so the behaviour stays the same and only the look changes.
- **Replace a screen.** Supply `screen-<id>.rml` (for example
  `screen-pause.rml`) with your own arrangement. Keep the ids that the player
  needs (see [Required elements](#required-elements)). Everything else in the
  markup is yours.
- **Add a screen.** Declare it in `design.json`, place it with
  `screen-order.rml`, and supply `screen-<id>.rml`. Disc adds a `disc` screen
  this way.

A design cannot change behaviour: what SAVE does, the number of slots (six),
or which screens have a special role. It can change every word and every
pixel.

## The files

| File | What it is | If absent |
|---|---|---|
| `design.json` | Declarations: fonts, screens, sizes, timings, words | Native's values |
| `menu.rcss` | Your stylesheet. It **replaces** Native's, so start from a copy of `native/menu.rcss` | Native's stylesheet |
| `screen-pause.rml`, `screen-controls.rml`, `screen-options.rml` | The markup of that screen | Native's |
| `screen-<list>.rml` | A frame around a generated list (Shaders, Achievements, the disc list) | the built-in frame |
| `save-slots.rml`, `footer.rml`, `spine.rml`, `overlay-splash.rml` | Parts of the page. Their text is what shows before the player writes its own words (see [Words](#words)). `#footer-hint` in `footer.rml` shows the `footer` of each screen. `overlay-splash.rml` is the startup splash, in the menu and on the logo-only page alike | Native's (Native has no spine) |
| `splash.rml` | The page of a game with a logo and no menu, with the places for the spine and the splash (`<!--SPINE-->`, `<!--SPLASH-->`) | Native's |
| `row.rml` | One row of every list | the built-in row |
| `option-entry.rml` | One Options entry | the built-in entry |
| `dialog-<name>.rml`, `actions-<screen>.rml` | A dialog that a screen opens, and extra buttons in the action row of a screen | Native's |
| `parts/slider.rml`, `parts/toggle.rml` | The volume slider, and a switch you place yourself | the shared parts |
| `achievements.rcss` | Extra rules for the achievements screens, after Native's | Native's only |
| `screen-order.rml` | Where your added screens go | no added screens |
| fonts and their licences | The files listed in `design.json` | Native's font |

The page skeleton, `native/menu.rml`, is fixed. We put every fragment above
into it. It contains the splash, the notice, the heading (`#heading`, with
the `heading` of each screen), the unlock pop-up and the insertion points.

Four shared stylesheets in `integrations/parts/` are linked **before** yours,
so any rule of yours wins over them:

- `navigation.rcss` marks which elements can take the focus.
- `slider.rcss` and `toggle.rcss` give the slider and the switch their basic
  look. They also hide a switch that the running game has no use for
  (`.disabled`), such as RUMBLE in a game whose core never rumbles a pad. A
  rule of yours that gives Options entries a display by id shows it again.
- with `background.rcss` we draw the author's background picture behind
  `#screen` when the game has one, and never while the game runs (`body.overlay`).

## design.json

Every key is optional (the name of the folder is the default `id`). We refuse
an unknown key, so a typo fails at export instead of being ignored. Disc's
file is a complete example.

```json
{
  "schemaVersion": 1,
  "id": "mydesign",
  "name": "My design",
  "description": "One line for people.",
  "documents": { "style": "menu.rcss", "splash": "splash.rml" },
  "fonts": [
    { "file": "MyFont.ttf", "license": "MyFont-OFL.txt", "family": "MyFont" }
  ],
  "screens": [
    { "id": "pause", "heading": "PAUSED", "footer": "ESC  RESUME" },
    { "id": "controls", "heading": "PAD", "option": { "label": "PAD", "default": true } }
  ],
  "words": { "slot": "FILE {slot}" },
  "tokens": { "ink": "#ffffff" }
}
```

- **`fonts`**: every font file the stylesheet uses, each with a licence file
  that ships with it. A design needs at least one font, because the player
  writes words with it. Name the family in RCSS with `font-family`.
- **`screens`**: merged with Native's by `id`. Give only the fields you
  change. The fields are:
  - `heading` and `footer`: the words shown while the screen is open.
    Pause's are in the composed menu too, because the menu opens on Pause.
  - `button`: the id of the element that opens the screen.
  - `label`: its words when another screen links to it.
  - `back`: the words on its BACK.
  - `option: {label, default}`: makes it an Options entry that the author
    may include. `null` removes the entry. With `at: "pause"`, your design
    places the entry's button on its pause screen, and we keep it there, or
    take it away when the author leaves the entry out. In a design that places
    no such button, we list the entry in Options before ABOUT. RESTART is such
    an entry.
  - `pageSize`: rows per page of its list, or entries per page on the
    Options screen.
  - `panel`: the id of its panel.
  - `place: "options"`: marks the Options screen itself.
  - `dialogs`: the dialogs it opens, each `dialog-<name>.rml`.
  - `from`: the screen whose button opens it, when that is not Pause or
    Options (QUICK SIGN IN opens from Achievements).
  - `images`: a list screen to open instead once the game has more than one
    disc (Disc's `disc` screen opens the disc list).

  A new `id` adds a screen. Only Native can set `role` (see
  [Screens and roles](#screens-and-roles)). The BACK of a screen is the
  button of the screen behind it, and we set those when we compose the
  menu, so leave `button` alone on Pause.
- **`screenOrder`**: the order of the screen declarations, when yours
  differs.
- **`metrics`**: the drawing on the pad screen.
  - `scene`: the box of the picture.
  - `marker`: the diameter of a button ring.
  - `callout`: the size and border of a label box.
  - `group`: the size, border, gap and `bottomMargin` of a stick box.

  We route the leader lines from these numbers, so they must match what your
  stylesheet draws.
- **`list`**: `pageSize`, `rowHeight` and `rowGap` for every list.
- **`binds`**: when the list of the bindings of a control appears, in ms
  after a key (`afterMs`) and after the pointer rests (`hoverAfterMs`), and
  its `width`.
- **`overlays`**: the timing of the splash and of the "PRESS ESC" notice (see
  [Overlays](#overlays)).
- **`documents`**: your `style` and `splash` files. Only Native can set
  `menu`, the page skeleton.
- **`words`** and **`tokens`**: see below.

## Screens and roles

Ten screens have a **role**, which the player code handles specially:

| Role | Screen | Contains |
|---|---|---|
| `pause` | Pause | the save slots and CONTINUE, SAVE, LOAD, RESTART, QUIT |
| `options` | Options | the entries, the volume and the settings |
| `controls` | the pad screen | the controller picture |
| `shaders` | the filter list | |
| `achievements` | the achievements screen | its sign-in form |
| `discs` | the disc list | |
| `accounts` | the saved accounts of QUICK SIGN IN | |
| `hotkeys` | HOTKEYS | a row of bindings for each hotkey |
| `forget` | UNINSTALL on Windows, RESET on a Mac | the question, then `forget-back` and `forget-confirm` |
| `restart` | RESTART | the question, then `restart-back` and `restart-confirm` |

Native assigns the roles. Your design keeps a role by keeping the id of the
screen: a `screen-pause.rml` is still Pause, whatever it looks like and
wherever `screenOrder` puts it. The menu always opens on Pause. BACK returns
to the screen from which the current one was opened.

**Every screen panel except Pause's** starts hidden (`style="display:none;"`)
and has the class `screen-panel`. We show one panel at a time.

A replaced **list frame** (`screen-shaders.rml` and the others) must keep
each of these exactly once:

- `id="PANEL-ID"` on a hidden element with class `screen-panel`
- `<!--ROWS-->`
- `<!--ACTIONS-->`
- `<!--STATUS-->`

We generate the rows, the paging, BACK and the status line into them.

**The button row of Pause** is finished when we compose the menu, because the
author chooses which Options entries a game has. When the game has Options,
one OPTIONS button (`#options`, with the `label` of the Options screen) takes
the place of the first button on your row that opens an Options entry
(Native's `#controls`), or goes before QUIT if there is none. The other entry
buttons leave the row, and their screens open from Options. When the game has
no Options, the entry buttons leave the row too, and `#actions` gets the class
`no-options`.

A replaced **Options** screen draws the panel declared in its `screens` entry
(Native's `options-panel`, or a `panel` of your own), with either
`<!--OPTIONS-->` or an element `id="options-entries"` in it. We put the
entries there as one list, `#options-list`, paged like every other list: its
`pageSize` entries to a page, and a pager when there is more than one page.
Place the list and its pager through the box you put it in
(`#options-entries .list`, `#options-entries .list-pager`), and give the
entries one height, so that a full page is where you drew it. We draw the
player's own settings in that panel too, without you naming them. To put one
somewhere else, write `<!--SETTING:volume-->` (or the id of another setting)
where it should go.

## Required elements

In the player we find each element we act on by id or class. The list is in
[`document_contract.inc`](../vendor/retroarch/menu/drivers/rmlui/document_contract.inc),
grouped by role. An element marked `Required` must be present whenever its
screen ships. We check every exported menu against this list and refuse one
with a missing element, with the name of the file and the element. A Pause
replacement needs the following:

- `#pause-panel`, `#status`, `#resume`, `#save`, `#load` and `#quit`.
- The six slots, `#slot-1` … `#slot-6`, each with class `slot`. They come from
  `save-slots.rml`. If you replace it, keep `slot-label-N`, `slot-image-N` and
  `slot-state-N` inside them. We draw the saved picture in `slot-image-N` and
  fill it. Native fills a `.slot-picture` marked to take the shape of the game
  (see [The game's shape](#the-games-shape)). Mark yours, or size it yourself.
- Class `menu-action` on the buttons.

**HOTKEYS** (`screen-hotkeys.rml`) has a row for each hotkey declared in
[`hotkeys.inc`](../vendor/retroarch/menu/drivers/rmlui/hotkeys.inc): `menu`,
`confirm`, `back`, `quick-save`, `quick-load`, `previous-slot` and
`next-slot`. Each row has `#hotkey-<hotkey>-add`, the + that captures
one more binding, and the chips `#hotkey-<hotkey>-1`, `-2` and on, each with
class `hotkey-chip`. A row holds as many bindings as you draw chips, and we
refuse an export whose defaults give a row more. In the player we hide an
unused chip, mark a chip with a key `key` and one with pad inputs `pad`,
disable the + of a full row, and mark the + of a running capture `capturing`.
`#hotkey-<hotkey>-label` is the
name of the hotkey wherever the menu reports a change to it. The screen also
needs `#hotkeys-back`, `#hotkeys-reset`, `#hotkeys-cancel` and
`#hotkeys-status`. Write the rows between `<!--ROWS-->` and `<!--/ROWS-->`.
At export we make them the list of the screen, `#hotkeys-list`, in pages of
the screen's `pageSize`, with the same page and pager as every list, and in
the player we page it as we page Options. Any screen may mark its rows this
way, once.

Native's own files are the working example for every screen. Copy the ids,
not the layout.

## Classes and attributes for your stylesheet

In the player we never position or colour anything. We set these classes and
attributes, and your RCSS decides what each one looks like:

| Set on | Meaning |
|---|---|
| `.focused` | the element with the focus (style this, never `:focus`, which also matches its parents) |
| `.selected` | the chosen item: the slot for SAVE and LOAD, or the chosen row |
| `.disabled` | not usable now (LOAD before anything is saved, or a player setting that the running game has no use for) |
| `.occupied`, `.empty` | a save slot with or without a save |
| `.on` | a switch that is on |
| `.dragging` | the slider while it is dragged |
| `.capturing` | a control waiting for a new input |
| `.showing`, `.leaving` | an overlay appearing and going |
| `.overlay` | on the body while the game runs with the menu closed |
| `data-notice="<notice>"` | on `#unlock-row`: the current notice, `achievement` (an achievement unlocked) or `slot` (the result of a hotkey for the save slots during play: `SAVED TO SLOT 3`, `LOADED SLOT 3`, `SLOT 3 IS EMPTY`, `SLOT 4`) |
| `.badge-loading` | a list row whose picture is still downloading |
| `.nav-outside` | everything outside an open dialog or picker |
| `data-focus="<id>"` | on the document: the id of the focused element |
| `data-game-shape="<shape>"` | on the document: the shape of the running game (below) |

With `data-focus` you can style anything by what has the focus. For example,
`[data-focus=save] .slot.selected` is the slot that SAVE is about to write.

An element with `data-fact="<name>"` shows a value that we keep current:
`chosen-slot` (the slot for SAVE and LOAD) and `saved-accounts` (the number of
accounts in QUICK SIGN IN). You decide where it goes:

```html
<button class="menu-action" id="save">SAVE · SLOT <span data-fact="chosen-slot">1</span></button>
```

### The game's shape

`data-game-shape` is the width of the running game divided by its height, to
three decimals: `1.333` for a 4:3 game, `1.778` for 16:9 and `1.111` for a
Game Boy. We read it each time the menu opens, so a game that changes shape
while it runs is followed. Style by it as by `data-focus`, for example
`[data-game-shape^="1.7"] .slot-label` for wide games.

To give an element the shape of the game, mark it `data-game-shaped` and give
it a `max-width` and a `max-height` as lengths. We then set its `width` and
`height`, in dp, to the shape of the game, as large as those limits allow, and
nothing else. They are the size of the content box, so a bevel or border goes
around the shape, and whatever fills the element fills it exactly, without
bars. Your own `width` and `height` are what a preview shows before the player
runs. We leave alone an element without a `max-width` or `max-height`, or with
a percentage, and a design that marks nothing stays as it is.

Where the element sits is up to you. Native puts the picture of each slot in a
frame as tall as the largest picture with its bevel and centres it there, so
the layout of the slot does not move with the game. Disc keeps the left edge
of the picture instead (`justify-content` left at its default):

```html
<div class="slot-frame"><div class="slot-picture" data-game-shaped><div id="slot-image-1" class="slot-image"/></div></div>
```

```css
.slot-frame { height: 142dp; display: flex; justify-content: center; align-items: center; }
.slot-picture { width: 184dp; height: 138dp; max-width: 230dp; max-height: 138dp; border: 2dp design(edge); }
.slot-image { width: 100%; height: 100%; }
```

We set two classes when we compose the menu for the exported game:

- `#actions.no-options`: the game has no Options screen.
- `#screen.with-background`: the author chose a background picture, shipped
  as `background.png`.

## Navigation

Pressing an arrow key or a direction on the d-pad moves the focus to the
nearest element that can take it, in that direction, on the laid-out screen. Moving the mouse
onto an element focuses it. Which elements can take the focus is declared
once, in `parts/navigation.rcss`: buttons with `menu-action`, slots, list
rows, Options entries, the slider, pagers, form fields and the controls on
the pad screen. You don't declare them.

- **The focus stops at the edges.** Down on the lowest element leaves the
  focus there.
- **Override one move** in your stylesheet with `nav-up`, `nav-down`,
  `nav-left` or `nav-right`, either `#some-id` or `none`. Disc uses
  `#quit { nav-down: none; }` to keep Down in its button column.
- **Choose the first focus of a screen** with the `autofocus` attribute on
  one element, as in `<button class="menu-action" id="resume" autofocus="autofocus">`,
  on any screen. Without it, the focus starts on the first element in
  document order that can take it, and on the pad screen on its first
  control instead of the picker.
- **Lists:** Left and Right on a row turn the page.

A design that draws screens in unusual places usually needs a few overrides.
With the navigation checks below you can see where the arrows go.

## Words

The words in your markup, and the headings, footers and labels in
`design.json`, are yours. In the player we also write some words ourselves,
such as `SLOT 1`, `EMPTY`, `SAVING SLOT 2...`, the `1/3` of a pager, the
messages of a capture on the pad screen, the mark on the running filter
(`shader-mark`) and on the disc in the tray (`disc-mark`), the result of a
hotkey for the save slots during play (`quick-saved`, `quick-loaded`,
`quick-slot-empty`, `quick-slot`), and the player settings in Options:
`volume`, `play-in-background`, `rumble`, the ends of a level (`level-low`,
`level-high`) and the state of a switch (`switch-on`, `switch-off`). Each has
an id and an English default in
[`words.inc`](../vendor/retroarch/menu/drivers/rmlui/words.inc). Give your own
in `design.json`:

```json
"words": { "slot": "FILE {slot}", "empty": "FREE", "saved-slot": "FILE {slot} WRITTEN" }
```

`{slot}` and the others are values that we fill in. Your wording may use the
values of the English wording and no others. We refuse an unknown id or value
at export. A word you don't give stays in English.

Keys are not words of a design. In the player and in the builder we word a
bound key as in
[`key_words.inc`](../vendor/retroarch/menu/drivers/rmlui/key_words.inc)
(`rshift` is `Right Shift`, `num1` is `1`), in the font of the design, so
every word there must exist in the font of every design.

A status line, `#status` on Pause and `#controls-status` on the pad screen,
shows the text of your markup, its prompt, whenever there is no message for
it. A message covers it for five seconds.

## Colours, palettes and values

Every design shares the palettes listed in `desktop/designs.json`. The author
picks one, and your design must look right in all of them. In RCSS, write
`design(name)` where a value goes, and we fill it in when we compose the menu:

- **Palette colours:** `screen`, `background`, `surface`, `picture`, `edge`,
  `highlight`, `muted` and `focus`, plus the `tokens` of each palette
  (`bevel-light`, `frame` and others).
- **Your own names:** the `tokens` in `design.json`, and Native's (`ink`),
  which every design inherits. A palette may override them.
- **Sizes:**
  - `canvas-width` and `canvas-height`, the 960 × 600 reference canvas.
  - The metrics: `scene-width`, `scene-height`, `marker-diameter`,
    `marker-radius`, `callout-width`, `callout-height`, `group-width` and
    `group-height`.
- **Other values:** `overlay-leave-<id>` (seconds) and `version`.

For example: `color: design(highlight); width: design(canvas-width)dp;`. We
refuse a name without a value at export.

Lay the menu out on the **960 × 600 dp canvas**. In the player we scale the
canvas to fit the window and keep its shape.

## Overlays

The splash (`#splash`) and the notice (`#notice`) show over the running game,
then leave. With `overlays` in `design.json` you set when each one appears
(`afterMs`), how long it stays (`holdMs`) and how long it takes to leave
(`leaveMs`). The element gets `showing`, then `leaving`. Animate the leaving
in RCSS for `design(overlay-leave-<id>)s`, so the motion and the timing come
from one number.

## Checking a design

**Draw it.** With this command we compose your design as an export does (for
a game with the default Options entries of your design), check it against the
contract, the word list and the `design(...)` values, and draw Pause into a
PNG. No window opens. You need to build the builder once first
(`uv run python scripts/build_builder.py`), which makes the renderer and the
runtime kit. The command below builds `rominabox-cli` when its sources are
newer.

```bash
echo '{"design":"'$PWD'/integrations/designs/mydesign","outputDir":"'$PWD'/work/preview-mydesign","palette":"blue"}' | "$(uv run python scripts/built.py)" preview
```

The picture is `work/preview-mydesign/preview.png`, with the composed
`menu.rml`, `menu.rcss` and `design.cfg` beside it. Draw each palette
(`blue`, `green`, `amber`, `carbon`, `violet`) into its own folder. An error
message contains the problem and the design, file or value. Your design folder
must be beside `native/`, because your design inherits from it.

The picture is the menu before the player runs. It has the heading and footer
of Pause, as the game opens on them, and the text of your markup where we
later write a word. It has no focus, no selection and no other screen. Your
headings, footers and words are in the composed `design.cfg`
(`screen_heading_pause`, `word_slot` and others). The checks below, and a
game, show the rest.

**Offer it.** Add `{"id": "mydesign", "name": "My design"}` to `designs` in
`desktop/designs.json`, and rebuild the builder so that it lists the design.
The test suite then covers it. For each check, this is what you record for
the new design:

| Check | Command | What to record for a new design |
|---|---|---|
| Composition and the contract | `uv run python scripts/test.py exporter` | a snapshot of the composed files: `ROMINABOX_RECORD_SNAPSHOT=1 cargo test --manifest-path desktop/crates/rominabox-engine/Cargo.toml --test menu_snapshot` |
| Where the arrows go on every screen | `uv run python scripts/test.py navigation` | the expected ids for your design in each table in `scripts/fixtures/navigation/`, written from what you see |
| Every recorded menu workflow | `uv run python scripts/test.py workflows` | an entry under `designs` in `scripts/fixtures/menu-workflows.json`, then its cases recorded with `ROMINABOX_WORKFLOW_RECORD=<case>,...` once you have checked them |
| Every state, drawn in every palette | `uv run python scripts/menu_states.py work/menu-states` | nothing to record, look at the pictures |

All of these run without a window or sound. With them you see where things
are and what the player does, but not whether it looks good, so look at the
pictures.
