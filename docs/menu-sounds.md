# Menu sounds

## One pack is one complete set

A pack is exactly one complete set of the four cues we play in the menu: `up`, `down`, `ok` and `cancel`. There are no variants, layers or sub-packs anywhere in the product. The author picks a pack, and that pack is the sound of the menu.

This rule is deliberate. The audition material in `work/menu-sound-candidates/` is organised as base voices plus alternative scroll shapes (`<voice>--soft`, `--tick`, `--dull`, `--orig-quiet`), so that we can judge movement cues apart from confirm cues. That structure is only an authoring aid, not a product concept. A shipped pack pairs one voice with one scroll shape, under one name.

The shipped packs, from `desktop/assets/menu-sounds/`:

| Pack | Character | Confirm voice | Movement cues |
| --- | --- | --- | --- |
| Blip | Plain 4-bit handheld pulse. The unadorned menu tick. | `dmg-blip` | `soft` scroll shape |
| Arpeggio | Quick note runs on confirm. The busiest, most chiptune set. | `dmg-arp` | `soft` scroll shape |
| Square | Thin, bright square-wave chip. Arcade cabinet, not FM. | `md-psg` | `soft` scroll shape |
| Arcade | Rising coin-op confirm with quick sweeps on move. | `md-arcade` | its own, re-levelled |
| Bell | Bright, musical FM bell. Clean and tuneful. | `md-bell` | `soft` scroll shape |
| Clang | Metallic, inharmonic FM. Sharp and aggressive. | `md-clang` | `soft` scroll shape |
| Thud | Low FM body under a bright tick. Chunky and bass-forward. | `md-thud` | `soft` scroll shape |
| Warm | Mellow FM at a low index. Rounded, with no hard edge. | `md-warm` | its own, re-levelled |

`off` is the ninth entry in the picker and the default. It is not an asset folder. It means that the cues of the pack (`audio_enable_menu_ok`, `_cancel`, `_scroll`) are false at export.

## Levels

Every cue in a pack, for moving, confirming or going back, is mastered to one level (`CUE_RMS`, matched on the loudest 30 ms), so a confirming click is never louder than a move. We check this in `themes::tests::every_cue_in_a_pack_is_equally_loud`. All cues are 44100 Hz, 16-bit, mono.

## Generating

`scripts/native_runtime/menu-sound-synthesis.mjs` contains the synthesis primitives and every voice. Both generators import it, so an approved audition file and the shipped pack are the same sound.

```bash
# Shipped packs -> desktop/assets/menu-sounds/, and soundPacks in desktop/designs.json
node scripts/native_runtime/generate-menu-sounds.mjs

# Audition material -> work/menu-sound-candidates/ (or a directory given as the argument)
node scripts/native_runtime/generate-menu-sound-candidates.mjs
```

To change the packs, edit `PACKS` in `generate-menu-sounds.mjs` and run it again. That writes the WAVs, removes the folders of retired packs, rewrites `PROVENANCE.txt`, and updates the `soundPacks` registry, which we read in both the Rust validator and the picker of the builder. Nothing is copied by hand.

With `scripts/kit_assets.py` we copy `desktop/assets/menu-sounds/` into the runtime kit at `resources/runtime/sound-packs/` and remove the packs that are no longer shipped, so nobody can select or bundle a retired pack.

## Runtime integration

The macOS build of RetroArch has `HAVE_AUDIOMIXER=1` and `HAVE_MENU=1`. In the menu of the player we use that mixer instead of a second audio system.

We stage the selected pack under `assets_directory/sounds/` in the exported runtime. In the mixer, the files `ok`, `cancel`, `up`, `down` and an optional looping `bgm` are found by their base names, with the extensions `ogg`, `mod`, `xm`, `s3m`, `mp3`, `flac` or `wav`. The mixer has dedicated system slots and its own menu volume and mute, and the BGM stops when the menu closes. The configuration booleans are `audio_enable_menu`, `audio_enable_menu_ok`, `audio_enable_menu_cancel`, `audio_enable_menu_bgm` and `audio_enable_menu_scroll`.

In the RmlUi driver we use one layer of focus and actions for every input source. Moving with the keyboard or a controller, and resting the pointer on an element, change the focus, and we request `up` or `down` once, only when the focus actually changes, with `down` for the pointer. A valid confirmation of Save, Load or Quit requests `ok`. When Load is unavailable, we report the existing error without a sound. Resume and Back request `cancel`. Resting the pointer on an element is not a click, so it only focuses the element and never triggers its action.

Upstream, the mixer has no selection of sound packs. To change the pack while a game runs, we would need a setting of our own and a controlled reload of the system streams of the mixer. The stock switch reloads the streams when menu audio is turned on, but has nothing for changing packs. So the pack is selected at export, and `off` is `audio_enable_menu=false`. Switches per event and BGM already exist upstream.

At export we set `audio_enable_menu_ok`, `audio_enable_menu_cancel` and `audio_enable_menu_scroll` to `true` for every selected pack and to `false` for `off`, set `audio_enable_menu_bgm` to `false`, and set `audio_enable_menu` to `true` for every game with a menu. We point `assets_directory` at the bundled `Resources/assets`, which contains `sounds/`. Only the selected pack ships, so an export contains four WAVs of 34 to 50 KB in total, never the whole library of 390 KB.

In every game that can change its volume, each step of a change makes one sound, at the new level (a drag plays one cue for each position it crosses). With a pack, it is the movement cue of the pack. With menu sounds Off, we ship the volume's own tick, `volume-tick.wav`, beside the menu. It is Blip's movement cue, the plain handheld pulse described as the unadorned menu tick, and we generate it with the packs into `integrations/parts/`. In the player we load it into the movement slots, where navigation still makes no sound, because `audio_enable_menu_scroll` stays false. To make the tick audible while the menu pauses the game, we set `audio_enable_menu` to true in every export with a menu, and the cues of the pack depend on the pack.

The audio of the builder's preview is separate. In the browser preview we play the same four asset URLs that the author selected, for the same action names. This cannot validate the loading in the RetroArch mixer, the volume, the pause, or the audio behaviour of a platform. Do not add a native helper player to the preview unless a later decision about the interface requires an exact check of native audio.

## Verified and unverified

With `cargo test --manifest-path desktop/crates/rominabox-engine/Cargo.toml --lib themes` we check that the declared packs and the shipped folders match exactly, that every pack has all four cues at 44100 Hz, 16-bit and mono, that every pack has a name and a character line, and that we reject the id of a retired pack before staging. With `npx vitest run src/MenuSoundPreview.test.tsx` we check the same set of packs from the builder side, and that a real Vite build emits the WAVs of every pack as hashed protocol assets instead of data URLs.

No automated test checks the native playback of these packs, or of the volume tick, in the exported player. In the `bridge` scope we check which cue the menu requests and at which level. Packaging and audio on Windows are still unverified.
