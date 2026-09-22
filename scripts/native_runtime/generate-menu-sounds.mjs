// The menu sound packs we ship. We made them by synthesis only, with no game
// recordings and no third-party audio.
//
// A pack is exactly one complete set of the four cues of the menu: up, down,
// ok and cancel. There are no variants, layers or sub-packs. Each pack below
// has a synthesised voice for its confirm cues (ok/cancel) and a scroll shape
// for its movement cues (up/down). That pair is the pack, and the person
// making a game sees only the pack.
//
// We build movement cues at the scroll level, about 10 dB below the confirm
// cues, because we play one in rmlui.c on every focus change and hover. For
// a voice with no scroll shape, `own` means its own up/down at that level.
//
// Write desktop/assets/menu-sounds/<id>/{up,down,ok,cancel}.wav, update the
// soundPacks registry in desktop/designs.json, and delete packs that we no
// longer ship. Run after changing PACKS:
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

/** Not an asset directory. At export we write audio_enable_menu=false.
 * The word Off is already in the dropdown, so we show no line under it. */
const OFF = { id: 'off', name: 'Off', description: '' };

const SOUNDS = fileURLToPath(new URL('../../desktop/assets/menu-sounds/', import.meta.url));
const REGISTRY = fileURLToPath(new URL('../../desktop/designs.json', import.meta.url));

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
    'One pack is one complete set of up, down, ok and cancel. Movement cues are mixed about 10 dB under',
    'the confirm cues because the menu plays one on every focus change and pointer hover.',
    '',
    ...PACKS.map((pack) => `${pack.id.padEnd(9)} ${pack.name.padEnd(9)} ${pack.voice} + ${pack.scroll} scroll — ${pack.description}`),
    '',
    'Project licensing remains to be selected.',
  ].join('\n') + '\n'
);

const registry = JSON.parse(await readFile(REGISTRY, 'utf8'));
registry.soundPacks = [
  OFF,
  ...PACKS.map(({ id, name, description }) => ({ id, name, description })),
];
await writeFile(REGISTRY, `${JSON.stringify(registry, null, 2)}\n`);

console.log(`\n${PACKS.length} packs, ${PACKS.length * CUES.length} samples -> ${SOUNDS}`);
console.log(`soundPacks refreshed in ${REGISTRY}`);
