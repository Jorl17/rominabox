// Synthesis code for the ROM-in-a-Box menu cues, with no game recordings and
// no third-party samples. We model two very different console voices: the Game
// Boy DMG (pulse/wave/LFSR noise, 4-bit) and the Mega Drive (YM2612-style FM
// plus SN76489-style PSG).
//
// We import this module in both generators, so the packs we ship and the
// candidates to listen to are the same sound:
//   generate-menu-sounds.mjs            -> desktop/assets/menu-sounds/ (shipped)
//   generate-menu-sound-candidates.mjs  -> work/menu-sound-candidates/ (audition)

export const SR = 44100;

/**
 * We master every cue in a pack, for moving, confirming or going back, to this
 * one level, so a click is never louder than a move. It is the level of the
 * movement cues, which we play on every focus change.
 */
export const CUE_RMS = 0.065;

// ---------------------------------------------------------------- primitives

/** Band-limited pulse, with harmonics only below Nyquist, so there is no aliasing. */
export function pulse(phase, duty, frequency) {
  const limit = Math.floor(SR / 2 / Math.max(frequency, 1));
  let sample = 0;
  for (let n = 1; n <= limit; n++) {
    sample += (4 / (n * Math.PI)) * Math.sin(n * Math.PI * duty) * Math.cos(2 * Math.PI * n * phase);
  }
  return sample;
}

/** Linear-feedback shift register built like the DMG/SN76489 noise channel. */
export function lfsr(shortMode) {
  let register = 0x7fff;
  return () => {
    const bit = (register ^ (register >> 1)) & 1;
    register = (register >> 1) | (bit << 14);
    if (shortMode) register = (register & ~0x40) | (bit << 6);
    return register & 1 ? -1 : 1;
  };
}

/** Chamberlin state-variable filter, for a resonant band-pass colour. */
export function svf(cutoff, q) {
  let low = 0;
  let band = 0;
  return (input, frequency = cutoff) => {
    const f = 2 * Math.sin((Math.PI * Math.min(frequency, SR / 2.5)) / SR);
    const high = input - low - (1 / q) * band;
    band += f * high;
    low += f * band;
    return band;
  };
}

export const lerp = (a, b, t) => a + (b - a) * t;
/** A geometric sweep sounds more musical than a linear one. */
export const glide = (a, b, t) => a * Math.pow(b / a, t);
export const softClip = (x, drive) => Math.tanh(x * drive) / Math.tanh(drive);
/** One-pole low-pass, to soften a cue that the player hears hundreds of times. */
export function lowpass(samples, cutoff) {
  const a = Math.exp((-2 * Math.PI * cutoff) / SR);
  let previous = 0;
  for (let i = 0; i < samples.length; i++) {
    previous = samples[i] * (1 - a) + previous * a;
    samples[i] = previous;
  }
  return samples;
}
/** Quantise to a signed n-bit grid, the DMG's audible 4-bit staircase. */
export const quantise = (x, bits) => {
  const levels = Math.pow(2, bits - 1) - 1;
  return Math.round(x * levels) / levels;
};

/**
 * Percussive envelope. With a near-instant attack and an exponential decay, a
 * cue sounds like a strike instead of a held tone, and so like a click instead
 * of a buzz.
 */
export function envelope(t, duration, { attack = 0.0015, curve = 4.5, hold = 0 } = {}) {
  if (t < attack) return t / attack;
  const after = t - attack - hold;
  if (after < 0) return 1;
  const span = Math.max(duration - attack - hold, 1e-6);
  return Math.exp((-curve * after) / span);
}

// ------------------------------------------------------------------- voices

/** DMG pulse channel, optionally swept, always quantised to 4 bits. */
export function dmgPulse({ from, to = from, duty = 0.5, duration, curve = 4.5, hold = 0 }) {
  const frames = Math.round(SR * duration);
  const out = new Float64Array(frames);
  let phase = 0;
  for (let i = 0; i < frames; i++) {
    const t = i / SR;
    const frequency = glide(from, to, i / frames);
    phase += frequency / SR;
    out[i] = quantise(pulse(phase, duty, frequency) * 0.35, 4) * envelope(t, duration, { curve, hold });
  }
  return out;
}

/** A run of pulse steps, the rapid arpeggio typical of chiptune music. */
export function dmgArpeggio({ notes, step, duty = 0.25, curve = 3 }) {
  const parts = notes.map((note, index) =>
    dmgPulse({
      from: note,
      duty,
      duration: step,
      curve: index === notes.length - 1 ? curve : 1.2,
    })
  );
  return concat(parts);
}

/** DMG noise channel. With shortMode the timbre is tighter and more metallic. */
export function dmgNoise({ clock, clockTo = clock, duration, shortMode = false, curve = 6 }) {
  const frames = Math.round(SR * duration);
  const out = new Float64Array(frames);
  const next = lfsr(shortMode);
  let held = next();
  let accumulator = 0;
  for (let i = 0; i < frames; i++) {
    const rate = glide(clock, clockTo, i / frames);
    accumulator += rate / SR;
    while (accumulator >= 1) {
      held = next();
      accumulator -= 1;
    }
    out[i] = held * 0.45 * envelope(i / SR, duration, { curve });
  }
  return out;
}

/**
 * Two-operator FM as on the YM2612. The modulator has a separate, faster
 * envelope, so the tone starts bright and then softens, and that change is
 * what distinguishes FM from a filtered square wave.
 */
export function fm({
  carrier,
  ratio = 1,
  index = 4,
  indexEnd = 0.4,
  duration,
  curve = 4.5,
  indexCurve = 7,
  detune = 0,
  carrierTo = carrier,
}) {
  const frames = Math.round(SR * duration);
  const out = new Float64Array(frames);
  let phase = 0;
  let modulatorPhase = 0;
  for (let i = 0; i < frames; i++) {
    const t = i / SR;
    const progress = i / frames;
    const frequency = glide(carrier, carrierTo, progress);
    const depth = lerp(indexEnd, index, Math.exp(-indexCurve * progress));
    modulatorPhase += (frequency * ratio + detune) / SR;
    phase += frequency / SR;
    const modulator = Math.sin(2 * Math.PI * modulatorPhase);
    out[i] = Math.sin(2 * Math.PI * phase + depth * modulator) * envelope(t, duration, { curve });
  }
  return out;
}

/** Noise through a resonant band-pass, then bit-crushed, for crunchy sounds. */
export function crunch({ centre, centreTo = centre, duration, q = 4.5, drive = 3, crushHz = 11025, curve = 6 }) {
  const frames = Math.round(SR * duration);
  const out = new Float64Array(frames);
  const next = lfsr(false);
  const filter = svf(centre, q);
  const holdEvery = Math.max(1, Math.round(SR / crushHz));
  let held = 0;
  for (let i = 0; i < frames; i++) {
    const progress = i / frames;
    const raw = next();
    const filtered = filter(raw, glide(centre, centreTo, progress));
    if (i % holdEvery === 0) held = filtered;
    out[i] = softClip(held * 0.9, drive) * envelope(i / SR, duration, { curve });
  }
  return out;
}

// ------------------------------------------------------------------ shaping

export function concat(parts) {
  const total = parts.reduce((sum, p) => sum + p.length, 0);
  const out = new Float64Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export function mix(...parts) {
  const length = Math.max(...parts.map((p) => p.length));
  const out = new Float64Array(length);
  for (const part of parts) for (let i = 0; i < part.length; i++) out[i] += part[i];
  return out;
}

/**
 * Remove DC, match loudness across packs, then fade the tail to avoid a click.
 *
 * Peak normalisation alone is not enough, because a sweep or a sparse tick is
 * near its peak only briefly, so it sounds several dB quieter than a dense
 * square wave with the same peak. Instead we match cues on the loudest 30ms
 * window, which is closer to how loud a short sound seems, and we soft-limit
 * the result instead of clipping it.
 */
export function master(samples, targetRms = CUE_RMS, ceiling = 0.85) {
  const mean = samples.reduce((sum, v) => sum + v, 0) / samples.length;
  for (let i = 0; i < samples.length; i++) samples[i] -= mean;

  const window = Math.min(Math.round(SR * 0.03), samples.length);
  let sum = 0;
  for (let i = 0; i < window; i++) sum += samples[i] * samples[i];
  let loudest = sum;
  for (let i = window; i < samples.length; i++) {
    sum += samples[i] * samples[i] - samples[i - window] * samples[i - window];
    loudest = Math.max(loudest, sum);
  }
  const rms = Math.sqrt(loudest / window);
  const gain = rms > 0 ? targetRms / rms : 0;

  const fade = Math.min(Math.round(SR * 0.004), Math.floor(samples.length / 4));
  for (let i = 0; i < samples.length; i++) {
    const tail = i > samples.length - fade ? (samples.length - i) / fade : 1;
    samples[i] = ceiling * Math.tanh((samples[i] * gain) / ceiling) * tail;
  }
  return samples;
}

/** 44100 Hz, 16-bit, mono, ready for the RetroArch mixer without conversion. */
export function wav(samples) {
  const buffer = Buffer.alloc(44 + samples.length * 2);
  buffer.write('RIFF');
  buffer.writeUInt32LE(buffer.length - 8, 4);
  buffer.write('WAVEfmt ', 8);
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(1, 20);
  buffer.writeUInt16LE(1, 22);
  buffer.writeUInt32LE(SR, 24);
  buffer.writeUInt32LE(SR * 2, 28);
  buffer.writeUInt16LE(2, 32);
  buffer.writeUInt16LE(16, 34);
  buffer.write('data', 36);
  buffer.writeUInt32LE(samples.length * 2, 40);
  for (let i = 0; i < samples.length; i++) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    buffer.writeInt16LE(Math.round(clamped * 32767), 44 + i * 2);
  }
  return buffer;
}

// ------------------------------------------------------------------- voices

/**
 * Every synthesised voice, by its audition ID. A voice is a complete set of
 * four cue builders. We make both the packs we ship and the candidates from
 * here, so an approved sound is the same in both.
 */
export const VOICES = {
  'dmg-blip': {
    console: 'Game Boy',
    character: 'Plain 50% pulse blips, 4-bit. The unadorned DMG menu tick.',
    cues: {
      up: () => dmgPulse({ from: 880, duration: 0.055 }),
      down: () => dmgPulse({ from: 587, duration: 0.055 }),
      ok: () => concat([dmgPulse({ from: 784, duration: 0.045, curve: 1 }), dmgPulse({ from: 1175, duration: 0.1 })]),
      cancel: () => concat([dmgPulse({ from: 392, duration: 0.05, curve: 1 }), dmgPulse({ from: 262, duration: 0.11 })]),
    },
  },
  'dmg-sweep': {
    console: 'Game Boy',
    character: 'Narrow 12.5% pulse with the hardware sweep unit running. Chirpy.',
    cues: {
      up: () => dmgPulse({ from: 660, to: 1320, duty: 0.125, duration: 0.05 }),
      down: () => dmgPulse({ from: 660, to: 330, duty: 0.125, duration: 0.05 }),
      ok: () => dmgPulse({ from: 523, to: 1568, duty: 0.125, duration: 0.095, curve: 3 }),
      cancel: () => dmgPulse({ from: 880, to: 220, duty: 0.125, duration: 0.14, curve: 3 }),
    },
  },
  'dmg-arp': {
    console: 'Game Boy',
    character: 'Rapid arpeggios on 25% pulse. Busiest of the Game Boy set.',
    cues: {
      up: () => dmgArpeggio({ notes: [784, 988], step: 0.026 }),
      down: () => dmgArpeggio({ notes: [988, 784], step: 0.026 }),
      ok: () => dmgArpeggio({ notes: [523, 659, 784, 1047], step: 0.028 }),
      cancel: () => dmgArpeggio({ notes: [659, 523, 392], step: 0.036 }),
    },
  },
  'dmg-tick': {
    console: 'Game Boy',
    character: 'LFSR noise in short mode. Dry, percussive, no pitch at all.',
    cues: {
      up: () => dmgNoise({ clock: 9000, duration: 0.028, shortMode: true }),
      down: () => dmgNoise({ clock: 5200, duration: 0.032, shortMode: true }),
      ok: () =>
        mix(
          dmgNoise({ clock: 12000, duration: 0.05, shortMode: true }),
          dmgPulse({ from: 1047, duration: 0.09, duty: 0.25, curve: 5 })
        ),
      cancel: () => dmgNoise({ clock: 3000, clockTo: 1200, duration: 0.11, curve: 4 }),
    },
  },
  'md-bell': {
    console: 'Mega Drive',
    character: 'Two-operator FM at a 3.5 ratio. Bright, bell-like, musical.',
    cues: {
      up: () => fm({ carrier: 880, ratio: 3.5, index: 4, duration: 0.07 }),
      down: () => fm({ carrier: 587, ratio: 3.5, index: 4, duration: 0.07 }),
      ok: () => fm({ carrier: 659, ratio: 3.5, index: 5.5, duration: 0.19, curve: 3.5 }),
      cancel: () => fm({ carrier: 330, ratio: 2.01, index: 5, duration: 0.2, curve: 3.5, detune: 1.7 }),
    },
  },
  'md-clang': {
    console: 'Mega Drive',
    character: 'Inharmonic FM at a high modulation index. Metallic and aggressive.',
    cues: {
      up: () => fm({ carrier: 740, ratio: 1.414, index: 8, duration: 0.06, curve: 6 }),
      down: () => fm({ carrier: 494, ratio: 1.414, index: 8, duration: 0.06, curve: 6 }),
      ok: () => fm({ carrier: 622, ratio: 1.414, index: 9, indexEnd: 1.2, duration: 0.15, curve: 4 }),
      cancel: () => fm({ carrier: 311, ratio: 2.83, index: 9, indexEnd: 1.5, duration: 0.19, curve: 4 }),
    },
  },
  'md-thud': {
    console: 'Mega Drive',
    character: 'Low FM body under a bright transient. Chunky and bass-forward.',
    cues: {
      up: () => mix(fm({ carrier: 165, ratio: 1, index: 4, duration: 0.08, curve: 6 }), dmgNoise({ clock: 14000, duration: 0.012, shortMode: true })),
      down: () => mix(fm({ carrier: 110, ratio: 1, index: 4, duration: 0.09, curve: 6 }), dmgNoise({ clock: 9000, duration: 0.012, shortMode: true })),
      ok: () => mix(fm({ carrier: 131, carrierTo: 196, ratio: 1, index: 5, duration: 0.17, curve: 4 }), fm({ carrier: 523, ratio: 2, index: 3, duration: 0.07, curve: 8 })),
      cancel: () => mix(fm({ carrier: 98, carrierTo: 73, ratio: 1, index: 5, duration: 0.21, curve: 3.5 }), dmgNoise({ clock: 2600, duration: 0.05, curve: 7 })),
    },
  },
  'md-psg': {
    console: 'Mega Drive',
    character: 'SN76489 square channel rather than FM. Thin, bright, arcade.',
    cues: {
      up: () => dmgPulse({ from: 1047, duration: 0.042, duty: 0.5, curve: 6 }),
      down: () => dmgPulse({ from: 698, duration: 0.042, duty: 0.5, curve: 6 }),
      ok: () => concat([dmgPulse({ from: 1047, duration: 0.035, curve: 1 }), dmgPulse({ from: 1568, duration: 0.085, curve: 5 })]),
      cancel: () => concat([dmgPulse({ from: 523, duration: 0.04, curve: 1 }), mix(dmgPulse({ from: 262, duration: 0.11, curve: 5 }), dmgNoise({ clock: 2200, duration: 0.07, curve: 6 }))]),
    },
  },
  'md-crunch': {
    console: 'Mega Drive',
    character: 'Band-passed noise, bit-crushed and saturated. The crunchy one.',
    cues: {
      up: () => crunch({ centre: 2600, duration: 0.038, q: 5, drive: 3 }),
      down: () => crunch({ centre: 1250, duration: 0.042, q: 5, drive: 3 }),
      ok: () => crunch({ centre: 1000, centreTo: 3200, duration: 0.095, q: 6, drive: 3.5, curve: 4.5 }),
      cancel: () => crunch({ centre: 700, centreTo: 420, duration: 0.14, q: 4, drive: 4.5, curve: 4 }),
    },
  },
  'dmg-soft': {
    console: 'Game Boy',
    character: 'Rounded 50% pulse, low-passed. Warm, not sharp.',
    cues: {
      up: () => lowpass(dmgPulse({ from: 740, duration: 0.06, curve: 3 }), 4000),
      down: () => lowpass(dmgPulse({ from: 494, duration: 0.06, curve: 3 }), 4000),
      ok: () => lowpass(concat([dmgPulse({ from: 659, duration: 0.05, curve: 1 }), dmgPulse({ from: 988, duration: 0.11, curve: 3.5 })]), 5000),
      cancel: () => lowpass(dmgPulse({ from: 440, to: 294, duration: 0.13, curve: 3 }), 4000),
    },
  },
  'dmg-chord': {
    console: 'Game Boy',
    character: 'Two pulses stacked into a chord. Fuller than one channel.',
    cues: {
      up: () => mix(dmgPulse({ from: 659, duration: 0.06 }), dmgPulse({ from: 988, duration: 0.06, duty: 0.25 })),
      down: () => mix(dmgPulse({ from: 523, duration: 0.06 }), dmgPulse({ from: 784, duration: 0.06, duty: 0.25 })),
      ok: () => mix(dmgPulse({ from: 523, duration: 0.16, curve: 3 }), dmgPulse({ from: 784, duration: 0.16, curve: 3, duty: 0.25 }), dmgPulse({ from: 1047, duration: 0.16, curve: 3, duty: 0.125 })),
      cancel: () => mix(dmgPulse({ from: 392, duration: 0.15, curve: 3.5 }), dmgPulse({ from: 262, duration: 0.15, curve: 3.5, duty: 0.25 })),
    },
  },
  'md-warm': {
    console: 'Mega Drive',
    character: 'Soft FM at a 2:1 ratio, low index. Mellow, rounded.',
    cues: {
      up: () => fm({ carrier: 660, ratio: 2, index: 2.5, duration: 0.07, curve: 4 }),
      down: () => fm({ carrier: 495, ratio: 2, index: 2.5, duration: 0.07, curve: 4 }),
      ok: () => fm({ carrier: 528, carrierTo: 792, ratio: 2, index: 3, duration: 0.18, curve: 3.5 }),
      cancel: () => fm({ carrier: 396, carrierTo: 264, ratio: 2, index: 3, duration: 0.2, curve: 3.5 }),
    },
  },
  'md-arcade': {
    console: 'Mega Drive',
    character: 'Bright PSG rise on confirm, quick blips on move. Coin-op feel.',
    cues: {
      up: () => dmgPulse({ from: 988, to: 1319, duration: 0.05, duty: 0.5, curve: 5 }),
      down: () => dmgPulse({ from: 784, to: 587, duration: 0.05, duty: 0.5, curve: 5 }),
      ok: () => concat([dmgPulse({ from: 784, duration: 0.03, curve: 1 }), dmgPulse({ from: 1047, duration: 0.03, curve: 1 }), dmgPulse({ from: 1568, duration: 0.1, curve: 4 })]),
      cancel: () => dmgPulse({ from: 622, to: 233, duration: 0.16, duty: 0.5, curve: 3.5 }),
    },
  },
};

// ----------------------------------------------------------- scroll voices

/**
 * Scroll cues require a different design from confirm cues. In rmlui.c we play
 * one on every focus change, including pointer hover, so the player hears them
 * hundreds of times for each confirm cue. They decay instead of stopping,
 * and are never in the piercing register.
 *
 * In each entry the timbre of a voice is one set of knobs for a scroll shape.
 */
export const scrollVoices = {
  'dmg-blip': (f, ms, bright) => dmgPulse({ from: f, duty: 0.5, duration: ms / 1000, curve: bright }),
  'dmg-arp': (f, ms, bright) => dmgPulse({ from: f, duty: 0.25, duration: ms / 1000, curve: bright }),
  'md-psg': (f, ms, bright) => dmgPulse({ from: f * 1.15, duty: 0.5, duration: ms / 1000, curve: bright }),
  'md-bell': (f, ms, bright, index) => fm({ carrier: f, ratio: 3.5, index: index, indexEnd: 0.2, duration: ms / 1000, curve: bright }),
  'md-clang': (f, ms, bright, index) => fm({ carrier: f, ratio: 1.414, index: index * 1.4, indexEnd: 0.3, duration: ms / 1000, curve: bright }),
  'md-thud': (f, ms, bright, index) => fm({ carrier: f * 0.32, ratio: 1, index: index, indexEnd: 0.2, duration: ms / 1000, curve: bright }),
};

/**
 * Four quiet scroll shapes, only as candidates. "orig-quiet" is the
 * candidate's own shape at the quieter level, so we can compare the change of
 * level apart from the change of shape.
 */
export const scrollShapes = {
  soft: { note: 'Longer with a real tail, lower pitch, gentle decay.', up: 659, down: 523, ms: 85, curve: 2.4, index: 2.2, cutoff: 7000 },
  tick: { note: 'Very short and very quiet. Nearly subliminal.', up: 784, down: 622, ms: 32, curve: 5.5, index: 2.8, cutoff: 9000 },
  dull: { note: 'Low-passed hard. No high edge left to fatigue you.', up: 523, down: 415, ms: 75, curve: 3, index: 1.6, cutoff: 2200 },
  'orig-quiet': { note: 'The original shape, just placed at the right level.', up: 880, down: 587, ms: 50, curve: 4.5, index: 4, cutoff: 12000 },
};

/**
 * Build one scroll cue for a voice at the common cue level.
 * `shape` is a key in `scrollShapes`, and `own` means the voice's own cue.
 */
export function scrollCue(voiceId, cue, shape) {
  if (shape === 'own') return master(VOICES[voiceId].cues[cue](), CUE_RMS);
  const bend = scrollShapes[shape];
  const voice = scrollVoices[voiceId];
  if (!bend) throw new Error(`Unknown scroll shape: ${shape}`);
  if (!voice) throw new Error(`No scroll voice for ${voiceId}; use 'own'.`);
  return master(lowpass(voice(bend[cue], bend.ms, bend.curve, bend.index), bend.cutoff), CUE_RMS);
}

/** Build one confirm cue for a voice at the common cue level. */
export function confirmCue(voiceId, cue) {
  return master(VOICES[voiceId].cues[cue](), CUE_RMS);
}

export const CUES = ['up', 'down', 'ok', 'cancel'];
