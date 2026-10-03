# System catalog and recognition

`desktop/systems.json` is the shared declaration for the builder, the native
engine, the command line and the exporter. It contains stable IDs, names and
aliases for people, file extensions, metadata catalogs, controller profiles,
the routing of cores and licences, and firmware requirements. A declared core
is a candidate for packaging. It does not mean that a runtime kit contains
that core, or that anyone has played a game with it. Before export, we check
the selected runtime kit and the required firmware.

The first catalog covers these groups:

- Nintendo: NES, SNES, Game Boy, Game Boy Color, Game Boy Advance, Nintendo 64,
  GameCube
- Sega: SG-1000, Master System, Game Gear, Mega Drive/Genesis, Sega CD,
  Dreamcast
- Sony: PlayStation and PlayStation 2
- Atari: 2600, 5200, 7800 and Lynx
- NEC: PC Engine/TurboGrafx-16 and PC Engine CD/TurboGrafx-CD
- SNK and Bandai handhelds: Neo Geo Pocket/Color and WonderSwan/Color

An extension that belongs to one system selects that system. For shared
containers, including `.cue`, `.chd`, `.iso` and `.m3u`, the system stays
open unless a recognized header is stronger evidence or the caller gives the
system explicitly. In particular, we never assume that a `.cue` file is a
PlayStation game, because Sega CD, PlayStation and PC Engine CD all use cue
sheets. The browser fallback and the native `inspect_game_with_system` API
both follow this rule. In the command line, an `inspect` request takes the
optional `system` field, and the Tauri command takes `systemOverride`.

We look up checksums in Libretro's No-Intro directory, which contains the
declared cartridge catalogs. Sega CD, PlayStation, PC Engine CD, Dreamcast,
GameCube and PlayStation 2 are not in that directory. For them, the file
names and the system stay editable, and exact disc metadata has to wait for a
provider with disc sets and track-level Redump hashes.

In the native detector we recognize the header signatures of iNES, Mega
Drive/Genesis, Game Boy/Color, Game Boy Advance, Nintendo 64, Atari 7800 and
Lynx. For the Game Boy we check the complete Nintendo logo and the cartridge
header checksum, and a `.gb` or `.gbc` extension alone does not select a
console. For a metadata lookup we read the game once and compute its CRC32
and SHA-1. For an iNES file we compute, in the same pass, the fingerprints of
the whole file and of the file without its 16-byte header, because the
checksum catalog stores ROM data without the header.

Catalog names and core capabilities follow the upstream
[Libretro database](https://github.com/libretro/libretro-database/tree/master/metadat/no-intro),
[core list](https://docs.libretro.com/guides/core-list/) and
[core license table](https://docs.libretro.com/development/licenses/). For its
Sega systems, the shared CD extensions and the required Sega CD region BIOS
names, the
[Genesis Plus GX documentation](https://docs.libretro.com/library/genesis_plus_gx/)
is the reference.
