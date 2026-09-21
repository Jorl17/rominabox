// Candidate menu sound packs to listen to and compare. The synthesis code is
// in menu-sound-synthesis.mjs, and we use it in generate-menu-sounds.mjs too,
// so a selected candidate sounds the same as the pack we ship.
//
// We write to work/menu-sound-candidates/, or to a directory given as the only
// argument, and never to desktop/assets/menu-sounds/. We build the selected
// packs with generate-menu-sounds.mjs.
import { mkdir, writeFile, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

import {
  CUES,
  SR,
  VOICES,
  confirmCue,
  scrollCue,
  scrollShapes,
  scrollVoices,
  wav,
} from './menu-sound-synthesis.mjs';

if (process.argv[2] === '-h' || process.argv[2] === '--help') {
  console.log('Usage: node scripts/native_runtime/generate-menu-sound-candidates.mjs [OUTPUT_DIRECTORY]');
  console.log('Replaces the contents of the output directory with every candidate voice and scroll shape.');
  process.exit(0);
}
const OUT = process.argv[2]
  ? path.resolve(process.argv[2])
  : fileURLToPath(new URL('../../work/menu-sound-candidates/', import.meta.url));

/** Voices with a reduced scroll timbre, so we can listen to other scroll shapes. */
const SCROLLABLE = Object.keys(scrollVoices);
const milliseconds = (samples) => Math.round((samples.length / SR) * 1000);

await rm(OUT, { recursive: true, force: true });
const manifest = [];
for (const [id, voice] of Object.entries(VOICES)) {
  const directory = path.join(OUT, id);
  await mkdir(directory, { recursive: true });
  const cues = {};
  for (const name of CUES) {
    const samples = confirmCue(id, name);
    await writeFile(path.join(directory, `${name}.wav`), wav(samples));
    cues[name] = milliseconds(samples);
  }
  manifest.push({ id, console: voice.console, character: voice.character, cues });
  console.log(`${id.padEnd(11)} ${voice.console.padEnd(11)} ${Object.entries(cues).map(([k, v]) => `${k} ${v}ms`).join('  ')}`);
}
const scrollManifest = [];
for (const voiceId of SCROLLABLE) {
  for (const [shape, bend] of Object.entries(scrollShapes)) {
    const directory = path.join(OUT, 'scroll', `${voiceId}--${shape}`);
    await mkdir(directory, { recursive: true });
    const cues = {};
    for (const cue of ['up', 'down']) {
      const samples = scrollCue(voiceId, cue, shape);
      await writeFile(path.join(directory, `${cue}.wav`), wav(samples));
      cues[cue] = milliseconds(samples);
    }
    scrollManifest.push({ pack: voiceId, variant: shape, note: bend.note, cues });
  }
}
await writeFile(path.join(OUT, 'scroll', 'manifest.json'), JSON.stringify(scrollManifest, null, 2));
console.log(
  `\n${scrollManifest.length} scroll variants (${SCROLLABLE.length} voices x ${Object.keys(scrollShapes).length}) at ~10 dB under the confirms`
);

await writeFile(path.join(OUT, 'manifest.json'), JSON.stringify(manifest, null, 2));
await writeFile(
  path.join(OUT, 'PROVENANCE.txt'),
  'Original ROM-in-a-Box menu sound candidates, synthesised by scripts/native_runtime/generate-menu-sound-candidates.mjs.\n' +
    'No game recordings and no third-party samples. Console voices are modelled from published hardware behaviour, not sampled.\n'
);
console.log(`\n${Object.keys(VOICES).length} candidates, ${Object.keys(VOICES).length * 4} samples -> ${OUT}`);
