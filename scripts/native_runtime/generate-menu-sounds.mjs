// The menu sound packs we ship. We made them by synthesis only, with no game
// recordings and no third-party audio.
//
// A pack is exactly one complete set of the four cues of the menu: up, down,
// ok and cancel. There are no variants, layers or sub-packs. Each pack below
// has a synthesised voice for its confirm cues (ok/cancel) and a scroll shape
// for its movement cues (up/down). That pair is the pack, and the person
// making a game sees only the pack.
//
// We master every cue to one level (CUE_RMS), so a click is as loud as a move.
// For a voice with no separate scroll shape, `own` means its own up/down.
//
// Write desktop/assets/menu-sounds/<id>/{up,down,ok,cancel}.wav, update the
// soundPacks registry in desktop/designs.json, delete packs that we no
// longer ship, and write the volume tick into the shared menu parts.
// Run after changing PACKS or VOLUME_TICK:
//   node scripts/native_runtime/generate-menu-sounds.mjs
import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

import { CUES, SR, VOICES, confirmCue, scrollCue, wav } from './menu-sound-synthesis.mjs';

/**
 * The selected packs, one per voice.
 *
 * `soft` is the scroll shape for every voice that has variants. For Arcade
 * and Warm we use their own movement cues at the scroll level.
 */
const PACKS = [
  {
    id: 'blip',
    name: 'Blip',
    description: 'Plain 4-bit handheld pulse. The unadorned menu tick.',
    voice: 'dmg-blip',
    scroll: 'soft',
  },
  {
    id: 'arpeggio',
    name: 'Arpeggio',
    description: 'Quick note runs on confirm. The busiest, most chiptune set.',
    voice: 'dmg-arp',
    scroll: 'soft',
  },
  {
    id: 'square',
    name: 'Square',
    description: 'Thin, bright square-wave chip. Arcade cabinet, not FM.',
    voice: 'md-psg',
    scroll: 'soft',
  },
  {
    id: 'arcade',
    name: 'Arcade',
    description: 'Rising coin-op confirm with quick sweeps on move.',
    voice: 'md-arcade',
    scroll: 'own',
  },
  {
    id: 'bell',
    name: 'Bell',
    description: 'Bright, musical FM bell. Clean and tuneful.',
    voice: 'md-bell',
    scroll: 'soft',
  },
  {
    id: 'clang',
    name: 'Clang',
    description: 'Metallic, inharmonic FM. Sharp and aggressive.',
    voice: 'md-clang',
    scroll: 'soft',
  },
  {
    id: 'thud',
    name: 'Thud',
    description: 'Low FM body under a bright tick. Chunky and bass-forward.',
    voice: 'md-thud',
    scroll: 'soft',
  },
  {
    id: 'warm',
    name: 'Warm',
    description: 'Mellow FM at a low index. Rounded, with no hard edge.',
    voice: 'md-warm',
    scroll: 'own',
  },
];

/**
 * The cue we play when the player changes the volume in a game exported with
 * menu sounds Off. It is the movement cue of the Blip pack, a plain handheld
 * pulse. In a game with a pack we play the pack's movement cue instead. The
 * cue belongs to the volume control, so we keep it with the shared menu parts
 * and include it only in a game without a pack.
 */
const VOLUME_TICK = { voice: 'dmg-blip', cue: 'up', scroll: 'soft' };

/** Not an asset directory. At export we write audio_enable_menu=false.
 * The word Off is already in the dropdown, so we show no line under it. */
const OFF = { id: 'off', name: 'Off', description: '' };

const SOUNDS = fileURLToPath(new URL('../../desktop/assets/menu-sounds/', import.meta.url));
const REGISTRY = fileURLToPath(new URL('../../desktop/designs.json', import.meta.url));
const PARTS = fileURLToPath(new URL('../../integrations/parts/', import.meta.url));

const SCROLL_CUES = new Set(['up', 'down']);
const milliseconds = (samples) => Math.round((samples.length / SR) * 1000);

const unknown = PACKS.filter((pack) => !(pack.voice in VOICES));
if (unknown.length) throw new Error(`Unknown voices: ${unknown.map((pack) => pack.voice).join(', ')}`);

await mkdir(SOUNDS, { recursive: true });
const shipped = new Set(PACKS.map((pack) => pack.id));
for (const entry of await readdir(SOUNDS, { withFileTypes: true })) {
  if (entry.isDirectory() && !shipped.has(entry.name)) {
    await rm(path.join(SOUNDS, entry.name), { recursive: true, force: true });
    console.log(`removed retired pack ${entry.name}`);
  }
}

for (const pack of PACKS) {
  const directory = path.join(SOUNDS, pack.id);
  await mkdir(directory, { recursive: true });
  const lengths = [];
  for (const cue of CUES) {
    const samples = SCROLL_CUES.has(cue)
      ? scrollCue(pack.voice, cue, pack.scroll)
      : confirmCue(pack.voice, cue);
    await writeFile(path.join(directory, `${cue}.wav`), wav(samples));
    lengths.push(`${cue} ${milliseconds(samples)}ms`);
  }
  console.log(`${pack.id.padEnd(9)} ${pack.voice.padEnd(10)} ${String(pack.scroll).padEnd(5)} ${lengths.join('  ')}`);
}

await writeFile(
  path.join(SOUNDS, 'PROVENANCE.txt'),
  [
    'Original ROM-in-a-Box menu cues, synthesised by scripts/native_runtime/generate-menu-sounds.mjs',
    'from scripts/native_runtime/menu-sound-synthesis.mjs. No game recordings and no third-party audio.',
    'Console voices are modelled from published hardware behaviour, not sampled. 44100 Hz, 16-bit, mono.',
    '',
    'One pack is one complete set of up, down, ok and cancel, all mastered to one level.',
    '',
    ...PACKS.map((pack) => `${pack.id.padEnd(9)} ${pack.name.padEnd(9)} ${pack.voice} + ${pack.scroll} scroll — ${pack.description}`),
    '',
    'Project licensing remains to be selected.',
  ].join('\n') + '\n'
);

{
  const tick = scrollCue(VOLUME_TICK.voice, VOLUME_TICK.cue, VOLUME_TICK.scroll);
  await writeFile(path.join(PARTS, 'volume-tick.wav'), wav(tick));
  await writeFile(
    path.join(PARTS, 'PROVENANCE.txt'),
    [
      'volume-tick.wav: an original ROM-in-a-Box cue, synthesised by scripts/native_runtime/generate-menu-sounds.mjs',
      `from scripts/native_runtime/menu-sound-synthesis.mjs (${VOLUME_TICK.voice} + ${VOLUME_TICK.scroll} scroll, ${VOLUME_TICK.cue}).`,
      'No game recordings and no third-party audio. 44100 Hz, 16-bit, mono.',
      '',
      'Project licensing remains to be selected.',
    ].join('\n') + '\n'
  );
  console.log(`volume tick ${milliseconds(tick)}ms -> ${PARTS}`);
}

const registry = JSON.parse(await readFile(REGISTRY, 'utf8'));
registry.soundPacks = [
  OFF,
  ...PACKS.map(({ id, name, description }) => ({ id, name, description })),
];
await writeFile(REGISTRY, `${JSON.stringify(registry, null, 2)}\n`);

console.log(`\n${PACKS.length} packs, ${PACKS.length * CUES.length} samples -> ${SOUNDS}`);
console.log(`soundPacks refreshed in ${REGISTRY}`);
