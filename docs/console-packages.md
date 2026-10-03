# Console packages

A console is one directory under `integrations/consoles/`. That directory contains `console.json` and everything the console introduces: controller profiles under `controllers/` and core components under `components/`. Another console uses them by id. It does not copy them, and it does not edit the package that contains them to register itself.

We generate `desktop/systems.json` and `desktop/controls.json` from these packages, so do not edit them by hand. We still read the generated files in the builder, the React interface and `rominabox-cli`. `npm run dev` and `npm run build` generate them again before anything else, which overwrites a manual edit. A test fails when the files in the repository differ from what we generate from the packages.

We return a catalog only when every package is valid. Otherwise we return every diagnostic we can, each with a stable `code`, the package and the field. We never fill in a missing declaration.

## What you have to add

When a console uses a component that another package already declares, and the built-in RetroPad profile, it needs only `console.json`. `integrations/consoles/wonderswancolor/` is such a package: it refers to `beetle_cygne`, declared by `wonderswan`, and to `retropad`, which is built into the catalog. The directory has no `controllers/` and no `components/`.

A console that introduces its own core and still uses RetroPad adds `components/` and nothing else. `atari2600` does this for `stella`.

A console that introduces pads and a core declares all of them. `megadrive` declares `genesis_plus_gx`, `megadrive` and `megadrive6`, and `segacd` uses those ids in its own `console.json`.

The shipped directories are named after the console id, but we don't require that. An error while reading a file contains the directory, and an error from comparing packages contains the console `id`. With directories named after the id, both are the same string.

## `console.json`

`schemaVersion` must be `1`. We refuse a version that this build does not know, so a new field is never half read. For the same reason, we refuse unknown fields on this object (`parse.invalid_json`).

| Field | Meaning |
| --- | --- |
| `id` | Stable id. At export it is part of the hash for the save directory (`stable_identity`), so a new id leaves existing player data behind. |
| `name` | The name shown to the author. |
| `aliases` | Other names for this console, matched without regard to case, together with `id`. We refuse an alias that equals the id or an alias of another console, because recognition would then depend on the load order. |
| `content.extensions` | The extensions by which we can recognise the console, without a leading dot. At least one is required. A console we can recognise is not necessarily one we can export. |
| `content.category` | `cartridge` or `disc` in every shipped package. We store the string and don't check its spelling. |
| `recognition.headerTitle` | Optional. One `{offset, length}` window, or a list of them when the title is not at one address. With `anchor`, the offset counts from an ASCII signature instead of byte 0, which is how we find the title of a disc inside a CHD. `magic` (`text` or `hex`, with its `offset`) must match, or we ignore the window. `complementAt` is the offset, from the window, of a little-endian checksum and its complement, and we ignore the window unless they add up to 0xFFFF. |
| `recognition.copierHeader` | Optional byte count of a copier header, present when the file size is that many bytes past a kilobyte boundary. Title offsets count from the start of the cartridge without that header. |
| `recognition.handlers` | Optional names of Rust routines for recognition that an offset cannot express. The package contains the name, not the algorithm. We don't check that a routine of that name exists, and we don't read this field in the inspector. NES still declares `ines_header`. In `metadata.rs` we select the hashing of NES files without a header by the `NES\x1a` magic, not by this list. |
| `metadata.catalog` | Optional checksum or serial catalogue, with `provider` and `name`. Cartridges use `no-intro`. Disc consoles use `redump`, matched by the serial inside the disc instead of a checksum of the image. |
| `cores` | Component ids in order of preference. Each entry is `{ "component": "<id>" }`. Some package must declare the id. The array may be empty. |
| `controllers.default` | The profile offered first. |
| `controllers.variants` | Every profile an author may pick, including the default. Only the console lists them, and a profile does not list consoles. We refuse a package where `variants` is not empty and leaves out `default`. Without `variants`, `default` is still the offered profile. |
| `support` | The intent for each target triple. See below. Without it, no intent is declared. |
| `presentationOrder` | Optional position in the author's console list. Consoles without it come after the numbered ones, by name. A new console can leave it out, and no other console needs a new number. |
| `firmware` | Optional groups of files that the author must supply before export. We do not ship, search for or download proprietary firmware. The names only identify a file the author already has. |

`wonderswancolor` is the package with one file. `wonderswan` declares `beetle_cygne`, and `retropad` is built in:

```json
{
  "schemaVersion": 1,
  "id": "wonderswancolor",
  "name": "WonderSwan Color",
  "aliases": [
    "wonderswan color"
  ],
  "content": {
    "extensions": [
      "wsc"
    ],
    "category": "cartridge"
  },
  "recognition": {},
  "metadata": {
    "catalog": {
      "provider": "no-intro",
      "name": "Bandai - WonderSwan Color"
    }
  },
  "cores": [
    {
      "component": "beetle_cygne"
    }
  ],
  "controllers": {
    "default": "retropad",
    "variants": [
      "retropad"
    ]
  },
  "support": {
    "macos-arm64": "planned"
  },
  "presentationOrder": 24
}
```

`megadrive` adds a header title, an enabled target, and two profiles of its own:

```json
{
  "schemaVersion": 1,
  "id": "megadrive",
  "name": "Mega Drive / Genesis",
  "aliases": [
    "mega drive",
    "genesis"
  ],
  "content": {
    "extensions": [
      "md",
      "gen",
      "smd"
    ],
    "category": "cartridge"
  },
  "recognition": {
    "headerTitle": {
      "offset": 336,
      "length": 48
    }
  },
  "metadata": {
    "catalog": {
      "provider": "no-intro",
      "name": "Sega - Mega Drive - Genesis"
    }
  },
  "cores": [
    {
      "component": "genesis_plus_gx"
    }
  ],
  "controllers": {
    "default": "megadrive",
    "variants": [
      "megadrive",
      "megadrive6"
    ]
  },
  "support": {
    "macos-arm64": "enabled"
  },
  "presentationOrder": 0
}
```

NES has a handler name and no header window:

```json
"recognition": {
  "handlers": [
    "ines_header"
  ]
}
```

Sega CD declares firmware. `minimum` is the number of `acceptedNames` that the author must supply:

```json
"firmware": [
  {
    "id": "segacd-bios",
    "acceptedNames": [
      "bios_CD_E.bin",
      "bios_CD_U.bin",
      "bios_CD_J.bin"
    ],
    "minimum": 1,
    "help": "Select at least one Sega CD region BIOS that you are entitled to use. ROM-in-a-Box does not search another emulator's files."
  }
]
```

A `minimum` of 0 means that the console can start without a file. PlayStation has 0, because its core includes a BIOS, and PC Engine CD and Sega CD have 1, because theirs do not. At export and in the builder we both use `assess_firmware`, the `firmware` command, so neither decides from `minimum` alone.

## Support intent

`support` is the intent for a target triple. It is not a report of which files happen to be on disk, and we never rewrite it when loading.

| Value | Meaning |
| --- | --- |
| `enabled` | This build is meant to ship the console. By the contract of the field, preparation fails when it cannot. |
| `planned` | Described and recognisable, but not claimed as shipped support. |
| `unsupported` | Deliberately not offered. |

Never record a missing file by changing `enabled` to `planned`, because that hides a console the build claims to ship. WonderSwan Color is `planned` because its package says so. Atari 2600 is `enabled` and uses RetroPad, and missing artwork is not what makes a console planned. No current preparation step reads `support`. It is still the value in the output of `rominabox-catalog list`, and we never derive it from the kit.

## Controller profiles

Each profile is one JSON file, directly under the `controllers/` directory of the package that declares it. We read only the `.json` files there. `schemaVersion` is `1`. The `id` must differ from the id of every other profile.

`retropad` is built into the catalog. Its presentation is generic, its name is `RetroPad`, and no package declares it. Do not declare it again: a file with the id `retropad` is `id.duplicate`, the same error as any other repeated profile id. Ten shipped consoles use it: both PC Engine packages, both WonderSwan packages, both Neo Geo Pocket packages, and the Atari 2600, 5200, 7800 and Lynx. We add it to the catalog once, so those consoles do not each declare a copy.

The `presentation` of a profile is one of:

- `{ "kind": "generic" }`: the control grid without artwork. It is a complete profile. It needs no artwork, and choosing it does not prevent `enabled`.
- `{ "kind": "illustrated", "image": "<file>" }`: a drawn pad. `image` is relative to the directory of the profile JSON, so `controller-megadrive.png` is next to `megadrive.json`. We refuse absolute paths, `..`, and a symlink that leads out of the package.

An illustrated profile whose PNG is missing is a broken package (`controller.illustration_missing`). We never fall back to generic. Generic is a presentation you declare, not a replacement for a missing file.

Illustrated controls need `x` and `y`, the centre of the button in the menu scene of 960×380 dp (`#controller-scene` in `integrations/designs/native/menu.rcss`). Without them the error is `controller.anchor_missing`. `calloutX` and `calloutY` are the label box in the same scene. We don't refuse a missing callout, and we then generate `0`. Generic profiles have none of the four coordinates. We compute the grid positions when we generate the registry: column `index % 4`, row `index / 4`, origin `(32, 20)`, stride `(232, 84)`.

`coreDevice`, when set, is the emulated device reported for the pad. A drawing of six buttons does not set that device. `megadrive` is `257`, `megadrive6` is `513` and `gamegear` is `769`.

`controls` is an ordered list. Each `id` must be one of the RetroPad ids, once: `up`, `down`, `left`, `right`, `b`, `a`, `y`, `x`, `l`, `r`, `l2`, `r2`, `select`, `start`, `l3`, `r3`. `label` is the console's name for that input, and `key` is the default keyboard binding. Mega Drive A is RetroPad `y`, B is `b` and C is `a`, which is why the label and the id differ.

A stick is the set of controls with the same `group`, drawn as one box with one ring. Each member has a `direction` (`up`, `right`, `down`, `left`, or `press` for the click), each once, in that order. `groups` gives the title of each stick, keyed by the group name: `{ "l_stick": { "title": "Control stick" }, "r_stick": { "title": "C-stick" } }`. We show that title in the box of the stick and in the table of the builder. A stick without a title is `controller.group_untitled`, and a title for a group that no control uses is `controller.group_unused`.

`presentationOrder` on a profile works as on a console: the numbered profiles first, then the rest by name. The built-in RetroPad comes near the end.

The Mega Drive 3-button profile, `integrations/consoles/megadrive/controllers/megadrive.json`:

```json
{
  "schemaVersion": 1,
  "id": "megadrive",
  "name": "Mega Drive · 3 buttons",
  "presentation": {
    "kind": "illustrated",
    "image": "controller-megadrive.png"
  },
  "controls": [
    {
      "id": "up",
      "label": "Up",
      "key": "up",
      "x": 333,
      "y": 160,
      "calloutX": 16,
      "calloutY": 16
    },
    {
      "id": "left",
      "label": "Left",
      "key": "left",
      "x": 312,
      "y": 181,
      "calloutX": 16,
      "calloutY": 100
    },
    {
      "id": "right",
      "label": "Right",
      "key": "right",
      "x": 354,
      "y": 181,
      "calloutX": 16,
      "calloutY": 268
    },
    {
      "id": "down",
      "label": "Down",
      "key": "down",
      "x": 333,
      "y": 203,
      "calloutX": 16,
      "calloutY": 184
    },
    {
      "id": "y",
      "label": "A",
      "key": "z",
      "x": 569,
      "y": 213,
      "calloutX": 744,
      "calloutY": 268
    },
    {
      "id": "b",
      "label": "B",
      "key": "x",
      "x": 623,
      "y": 186,
      "calloutX": 744,
      "calloutY": 184
    },
    {
      "id": "a",
      "label": "C",
      "key": "c",
      "x": 678,
      "y": 164,
      "calloutX": 744,
      "calloutY": 100
    },
    {
      "id": "start",
      "label": "Start",
      "key": "enter",
      "x": 587,
      "y": 111,
      "calloutX": 744,
      "calloutY": 16
    }
  ],
  "presentationOrder": 0,
  "coreDevice": 257
}
```

The built-in RetroPad is generic. It has no image and no anchors, and it is not a file in any package: we add it to the catalog, so consoles without a pad of their own can use it. Game Gear is illustrated, in `integrations/consoles/gamegear/controllers/gamegear.json`, like the other handhelds.

In the catalog we check the PNG next to the profile JSON. In the macOS builder script we stage the illustrations from `desktop/assets/controllers/`, with the file names printed by `rominabox-catalog assets`. An illustrated profile needs the file in both places. `CONTROLLERS.txt` in that assets directory records the provenance of the artwork, and the catalog does not read it.

## Core components

Each component is one JSON file, directly under the `components/` directory of the package that declares it. `schemaVersion` is `1`. The `id` must differ from the id of every other component.

`artifacts` maps a target to the file name of that build inside a prepared kit. The targets of a component are `macos-arm64`, `macos-x86_64`, `windows-x86_64` and `linux-x86_64`, and the file name ends in `.dylib`, `.dll` or `.so` to match. `license.spdx` is an SPDX id, or the exact terms when SPDX has no id for them. Genesis Plus GX uses `Non-commercial`, a limit on the field of use. `license.file` is the file name of the licence text that we stage into an export.

`capabilities` lists what this build can do. In the exporter we read this list, not the feature list of the upstream project. An empty list means that no constraint is recorded. Cartridge cores ship that way, and we then refuse no container at export because of a capability. We check a non-empty list when the ROM extension is one of `chd`, `cue`, `iso`, `gdi`, `cdi`, `pbp`, `rvz` or `m3u`, and refuse the export when the extension is not in `capabilities`. Genesis Plus GX declares `cue` and `iso`, so we refuse a `.chd` even though the upstream project can read CHD. Beetle PCE Fast declares `chd` as well, because that artifact was built with it.

`libraryName` is the libretro library name reported by the artifact. RetroArch names two directories per core after it: the one for the controller remap, which every game ships while every pad is player 1, and the one for the options file. Every core declares one. Read it from the artifact, with `frame_harness --frames 1`. When it is guessed from the component id, a remap can end up in a directory that nothing reads.

`pixels` is optional. Each entry is `{ "key", "value" }`, a core option that keeps the core from replacing its pixel buffer with a blended reconstruction. At export we write these into `<libraryName>/<libraryName>.opt` and leave every other option at the default declared by the core. The key and the value are the core's own tokens, and we don't interpret them in the exporter. Leave the field out when those defaults already keep the pixels intact. The declared default of Nestopia is its composite-video filter, so its component sets that option to `disabled`. Genesis Plus GX has the same filter and already defaults it to `disabled`, so it declares nothing.

`provenance` is optional, and we accept a component without it in the catalog. In `scripts/prepare_runtime.py` we stage only the components that have it. The `origin` `built` means that we compile it where its recipe covers the machine, and `libretro-buildbot` that we download a nightly. We refuse any other origin. `repository` and `revision` identify the source snapshot that we download during preparation. `branch` is the branch whose current tip contains the licence text that goes with a downloaded core. It is not `revision`, which is a commit. `licenseCandidates` are the paths to try, in order, when extracting the licence. We record no hash of a nightly or of its licence text, because the buildbot directory is `latest`, which is replaced in place. We generate the download list of the builder, `desktop/core-pins.json`, from the components: for each platform, the file name of each artifact inside the nightly archive, and the branch with the licence. We accept a download when it succeeds and the archive contains that file name. `prepare_runtime.py --seed-core-cache` fills the developer core cache with that same download (`rominabox-cli cores`). `correspondsToArtifact` is true when that snapshot is the source of this binary, and false when we keep the snapshot for the licence text and don't know whether the binary came from that revision. Genesis Plus GX is `built` with `correspondsToArtifact` true, and its nightly is still in the download list, so we can fetch it at export when the compiled file is not in the kit. Stella is `libretro-buildbot` with `correspondsToArtifact` false.

`integrations/consoles/megadrive/components/genesis_plus_gx.json`:

```json
{
  "schemaVersion": 1,
  "id": "genesis_plus_gx",
  "name": "genesis plus gx",
  "artifacts": {
    "macos-arm64": "genesis_plus_gx_libretro.dylib"
  },
  "license": {
    "spdx": "Non-commercial",
    "file": "genesis_plus_gx.txt"
  },
  "capabilities": [
    "cue",
    "iso"
  ],
  "provenance": {
    "origin": "built",
    "repository": "ekeeke/Genesis-Plus-GX",
    "revision": "27426f00aa68f9f358c86919e8a40985326fa05b",
    "licenseCandidates": [
      "LICENSE.txt"
    ],
    "correspondsToArtifact": true
  }
}
```

With `generate` we write every artifact declared by the component into `systems.json`. A consumer asks for the target it runs on or builds for.

## Commands

From `desktop/`:

```sh
npm run catalog
npm run catalog:validate
```

With `catalog` we generate `desktop/systems.json` and `desktop/controls.json` again. With `catalog:validate` we load the packages, with an error exit when there is any diagnostic. `npm run dev` and `npm run build` start with `catalog`.

The same tool from the repository root:

```sh
cargo run --quiet --manifest-path desktop/crates/rominabox-catalog/Cargo.toml --bin rominabox-catalog -- <command>
```

The optional second argument is the package root. The default is `integrations/consoles` in this repository. The output paths of `generate` are always in the `desktop/` of this repository, because we derive them from the crate directory, not from the current working directory. Without a command, the tool does `validate`.

| Command | Result |
| --- | --- |
| `validate` | Prints the numbers of consoles, profiles and components when the root is valid, with the built-in `retropad` among the profiles. On failure, prints every diagnostic to stderr and exits with an error. |
| `list` | One line per console, sorted by id: the id, the default profile, then `target=Enabled`, `Planned` or `Unsupported` from the `support` map. |
| `assets` | The illustration file names to stage, one per line, sorted and without duplicates. Generic profiles are left out. |
| `components` | Pretty-printed JSON with the id, artifacts, licence, capabilities and provenance of each component, which we read in `prepare_runtime.py`. |
| `generate` | Writes `desktop/systems.json`, `desktop/controls.json` and `desktop/core-pins.json`. The download list has one file name per platform and the branch with the licence. |

For an unknown command, the output lists the five commands above, with an error exit.

## Adding a console

1. Create `integrations/consoles/<id>/console.json` with `schemaVersion` 1, at least one extension, and a `controllers.default` that exists. Use `retropad` unless this console has its own pad. If you don't introduce a component, point `cores` at the id of an existing one. Set `support` explicitly. Leave out `presentationOrder` unless the console must be at a chosen position.
2. To introduce a core, add `components/<component-id>.json` to the package that declares it. Record `capabilities` from the artifact you will actually ship. Set `provenance` if preparation should stage it, and set `correspondsToArtifact` according to whether that source built the binary.
3. To introduce a profile, add `controllers/<profile-id>.json`, with any id except `retropad`. For `illustrated`, put the PNG next to that JSON and set `x` and `y` on every control. Copy the PNG, with the same file name, into `desktop/assets/controllers/`, so we can stage it in the macOS builder.
4. A second console that uses the new core or profile refers to its id in its own `console.json`. Leave the files of the package that declares it alone.
5. From `desktop/`, run `npm run catalog`, so the generated registries match the packages.
6. Run `npm run catalog:validate`.
7. Run the catalog tests. In `tests/extension.rs` we load a console unknown to the crate from its directory alone, including one `console.json` that uses `retropad` and a guest package that reuses the profile and component of another package. In `tests/invalid_packages.rs` we fix the diagnostic code, package and field for broken packages. In `tests/parity.rs` we require the generated registries to match the packages byte for byte:

```sh
cargo test --manifest-path desktop/crates/rominabox-catalog/Cargo.toml
```

The drift test is `the_checked_in_registries_are_what_the_catalog_generates` in `desktop/crates/rominabox-catalog/tests/parity.rs`. When it fails, generate the files again with `npm run catalog`, or fix the package. Do not edit `systems.json` or `controls.json` to make it pass.

## Diagnostics

We print diagnostics as `package [code] field: message`, sorted by package and then by field. We return no catalog when there is any diagnostic, so a missing profile never quietly becomes `retropad`.

A `schemaVersion` that this build does not understand, or a duplicate console id, stops the reading of that package. We still check the other packages. `root.unreadable` comes alone.

| Code | Meaning |
| --- | --- |
| `root.unreadable` | We could not read the package root. `package` is that path, and `field` is empty. |
| `package.no_manifest` | A directory under the root has no `console.json`. `field` is `console.json`. |
| `parse.invalid_json` | The file is not JSON, or it does not match the struct (including an unknown field in a document where unknown fields are refused). `field` is `console.json`, `controllers/<file>` or `components/<file>`. |
| `schema.unsupported_version` | `schemaVersion` is not `1`, on a console, a profile or a component. `field` is `schemaVersion`. |
| `id.duplicate` | The same console id, profile id or component id is declared twice. `field` is `id`, `controllers` or `components`. Declaring `retropad` again gives `controllers`. The package in the message is the one loaded later. |
| `content.no_extensions` | `content.extensions` is empty. `field` is `content.extensions`, and `package` is the console id. |
| `alias.duplicate` | An alias, or the id of this console, matches the id or an alias of another console, without regard to case. `field` is `aliases`. |
| `reference.missing_component` | A `cores` entry refers to a component that no package declares. `field` is `cores`. |
| `reference.missing_profile` | `controllers.default` or a variant refers to a profile that nobody declares, including a typo for `retropad`. `field` is `controllers`. |
| `controller.default_not_offered` | `variants` is not empty and does not contain `default`. `field` is `controllers.default`. |
| `control.unknown_id` | A control `id` is not in the RetroPad list above. `field` is `<profile>.controls`. |
| `control.duplicate_id` | The same control `id` appears twice in one profile. `field` is `<profile>.controls`. |
| `asset.escapes_package` | An illustration path is absolute, contains `..`, or leads outside the directory of the profile. `field` is `<profile>.presentation.image`. |
| `controller.illustration_missing` | The `image` of an illustrated profile is not a file. `field` is `<profile>.presentation.image`. |
| `controller.anchor_missing` | An illustrated control has no `x` or no `y`. `field` is `<profile>.controls.<control id>`. |
| `controller.group_untitled` | A stick has no title, or a blank one, in `groups`. `field` is `<profile>.groups.<group>`. |
| `controller.group_unused` | `groups` has a title for a group that no control uses. `field` is `<profile>.groups.<group>`. |
