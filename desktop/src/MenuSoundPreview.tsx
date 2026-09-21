import { useEffect, useRef, useState } from "react";
import { Play, Square } from "lucide-react";

import grainCancel from "../assets/menu-sounds/grain/cancel.wav";
import grainDown from "../assets/menu-sounds/grain/down.wav";
import grainOk from "../assets/menu-sounds/grain/ok.wav";
import grainUp from "../assets/menu-sounds/grain/up.wav";
import pulseCancel from "../assets/menu-sounds/pulse/cancel.wav";
import pulseDown from "../assets/menu-sounds/pulse/down.wav";
import pulseOk from "../assets/menu-sounds/pulse/ok.wav";
import pulseUp from "../assets/menu-sounds/pulse/up.wav";
import "./MenuSoundPreview.css";

const CUES = ["up", "down", "ok", "cancel"] as const;
type Cue = (typeof CUES)[number];

/** One short navigation / confirm / cancel demonstration. */
const PREVIEW_SEQUENCE: readonly Cue[] = ["up", "down", "ok", "cancel"];

const SOUND_URLS = {
  pulse: {
    up: pulseUp,
    down: pulseDown,
    ok: pulseOk,
    cancel: pulseCancel,
  },
  grain: {
    up: grainUp,
    down: grainDown,
    ok: grainOk,
    cancel: grainCancel,
  },
} as const satisfies Record<string, Record<Cue, string>>;

function previewUrls(pack: string): Record<Cue, string> | undefined {
  if (!(pack in SOUND_URLS)) return undefined;
  return SOUND_URLS[pack as keyof typeof SOUND_URLS];
}

function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function usesObjectUrl(url: string): boolean {
  if (
    url.startsWith("data:") ||
    url.startsWith("tauri:") ||
    url.startsWith("asset:")
  )
    return true;
  const protocol = globalThis.location?.protocol;
  if (protocol === "tauri:" || protocol === "asset:") return true;
  const origin = globalThis.location?.origin ?? "";
  return origin.startsWith("tauri:") || origin.startsWith("asset:");
}

type Playable = {
  src: string;
  release: () => void;
};

async function resolvePlayable(url: string): Promise<Playable> {
  if (!usesObjectUrl(url)) return { src: url, release() {} };

  const response = await fetch(url);
  if (!response.ok) throw new Error("unavailable");
  const src = URL.createObjectURL(
    new Blob([await response.arrayBuffer()], { type: "audio/wav" }),
  );
  return {
    src,
    release() {
      URL.revokeObjectURL(src);
    },
  };
}

type Session = {
  token: number;
  audio: HTMLAudioElement | null;
  finish: ((error?: unknown) => void) | null;
};

function release(audio: HTMLAudioElement | null): void {
  if (!audio) return;
  audio.onended = null;
  audio.onerror = null;
  audio.pause();
  audio.removeAttribute("src");
  audio.load();
}

function invalidate(session: Session): void {
  session.token += 1;
  session.finish?.();
}

async function playCue(
  session: Session,
  token: number,
  url: string,
): Promise<void> {
  if (token !== session.token) return;

  const playable = await resolvePlayable(url);
  if (token !== session.token) {
    playable.release();
    return;
  }

  await new Promise<void>((resolve, reject) => {
    const audio = new Audio(playable.src);
    session.audio = audio;

    const finish = (error?: unknown) => {
      if (session.finish !== finish) return;
      session.finish = null;
      session.audio = null;
      release(audio);
      playable.release();
      if (error) reject(error);
      else resolve();
    };

    session.finish = finish;
    audio.onended = () => finish();
    audio.onerror = () => finish(new Error("unavailable"));
    void audio.play().then(
      () => {
        if (token !== session.token) finish();
      },
      (cause) => {
        if (isAbort(cause) || token !== session.token) finish();
        else finish(cause);
      },
    );
  });
}

export function MenuSoundPreview({ pack }: { pack: string }) {
  const urls = previewUrls(pack);
  const [playing, setPlaying] = useState(false);
  const [error, setError] = useState("");
  const playingRef = useRef(false);
  const sessionRef = useRef<Session>({ token: 0, audio: null, finish: null });
  const seenPack = useRef(pack);

  if (seenPack.current !== pack) {
    seenPack.current = pack;
    sessionRef.current.token += 1;
    playingRef.current = false;
  }

  useEffect(() => {
    setPlaying(false);
    setError("");
    return () => {
      invalidate(sessionRef.current);
      playingRef.current = false;
    };
  }, [pack]);

  const available = urls !== undefined;
  const label = playing ? "Stop" : "Preview menu sounds";

  function stopPreview(): void {
    invalidate(sessionRef.current);
    playingRef.current = false;
    setPlaying(false);
  }

  function togglePreview(): void {
    if (playingRef.current) {
      stopPreview();
      return;
    }
    if (!urls) return;
    setError("");
    playingRef.current = true;
    setPlaying(true);
    invalidate(sessionRef.current);
    void runPreview(sessionRef.current.token, urls);
  }

  async function runPreview(
    token: number,
    packUrls: Record<Cue, string>,
  ): Promise<void> {
    const session = sessionRef.current;
    try {
      for (const cue of PREVIEW_SEQUENCE) {
        if (token !== session.token) return;
        await playCue(session, token, packUrls[cue]);
      }
      if (token === session.token) {
        playingRef.current = false;
        setPlaying(false);
      }
    } catch (cause) {
      if (token !== session.token || isAbort(cause)) return;
      invalidate(session);
      playingRef.current = false;
      setPlaying(false);
      setError("Could not play.");
    }
  }

  return (
    <span className="menu-sound-preview">
      <button
        type="button"
        className="icon-button"
        aria-label={label}
        title={label}
        disabled={!available}
        onClick={togglePreview}
      >
        {playing ? <Square size={18} /> : <Play size={18} />}
      </button>
      <span className="menu-sound-preview-status" role="status">
        {error}
      </span>
    </span>
  );
}
