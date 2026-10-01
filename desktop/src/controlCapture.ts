import { useEffect, useRef, useState } from "react";
import { CAPTURE_SECONDS, listen, type Pressed } from "./bindingCapture";
import { moveTo, positionName, withPads } from "./padPositions";

type Control = { id: string; label: string; key: string };
type Binding = { key?: string; pad?: string };
export type Bindings = Record<string, Binding | undefined>;

/** A capture under way: the controls to bind, in order, and the current one. */
export type Capture = {
  /** The Bind button that started it: a control, or a stick's group. */
  stop: string;
  members: string[];
  step: number;
  seconds: number;
};

/**
 * Binding by pressing, as on the Controls screen of the game: one wait per
 * control for a key or a pad press, and the directions of a stick one after
 * another in their declared order, each saved as soon as it is pressed. We
 * do not take an input that the hotkeys reject (`refuses`). For a key that
 * another control uses, we ask first. We swap a pad position with the control
 * that had it, and the author cannot take a position that another offered
 * pad uses. Stopping partway keeps what was pressed, and we say so.
 */
export function useControlCapture({
  controls,
  fixed,
  bindings,
  setKey,
  movePads,
  refuses,
  say,
  ended,
}: {
  controls: Control[];
  fixed: { id: string }[];
  bindings: Bindings;
  setKey: (id: string, key: string) => void;
  movePads: (moved: Record<string, string | undefined>) => void;
  /** Why we reject the controls bound as `bindings` because of the hotkeys,
   * or null. */
  refuses: (bindings: Bindings) => Promise<string | null>;
  say: (message: string) => void;
  /** Called when a capture is over, to put focus back on its button. */
  ended: (stop: string) => void;
}) {
  const [capture, setCapture] = useState<Capture | null>(null);
  const [pending, setPending] = useState<{
    key: string;
    conflicts: string[];
  } | null>(null);
  const latest = useRef({ capture, bindings, controls, fixed });
  latest.current = { capture, bindings, controls, fixed };
  const label = (id: string) =>
    latest.current.controls.find((control) => control.id === id)?.label ?? id;

  function end(why: "done" | "stopped", also = "") {
    const current = latest.current.capture;
    if (!current) return;
    const saved = current.members.slice(0, current.step).map(label);
    setCapture(null);
    setPending(null);
    const words =
      why === "done"
        ? "Binding updated."
        : saved.length
          ? `Saved ${list(saved)}.`
          : "Binding unchanged.";
    say(also ? `${also} ${words}` : words);
    ended(current.stop);
  }
  function advance() {
    const current = latest.current.capture;
    if (!current) return;
    if (current.step + 1 < current.members.length)
      setCapture({
        ...current,
        step: current.step + 1,
        seconds: CAPTURE_SECONDS,
      });
    else end("done");
  }
  /** Whether we still wait for the control of `current` after an answer. */
  function stillWaiting(current: Capture) {
    const now = latest.current.capture;
    return !!now && now.stop === current.stop && now.step === current.step;
  }
  async function take(input: Pressed) {
    const { capture: current, bindings: now, controls: all } = latest.current;
    if (!current) return;
    const member = current.members[current.step];
    if (input.kind === "key") {
      const refused = await refuses({
        ...now,
        [member]: { ...now[member], key: input.key },
      });
      if (!stillWaiting(current)) return;
      if (refused) {
        end("stopped", refused);
        return;
      }
      const conflicts = all
        .filter(
          (control) =>
            control.id !== member &&
            (now[control.id]?.key ?? control.key) === input.key,
        )
        .map((control) => control.label);
      if (conflicts.length) {
        setPending({ key: input.key, conflicts });
        return;
      }
      setKey(member, input.key);
      advance();
      return;
    }
    const moved = moveTo(
      all,
      latest.current.fixed,
      now,
      member,
      input.position,
    );
    if (!moved) {
      end("stopped", `${positionName(input.position)} is taken.`);
      return;
    }
    const refused = await refuses(withPads(now, moved));
    if (!stillWaiting(current)) return;
    if (refused) {
      end("stopped", refused);
      return;
    }
    movePads(moved);
    advance();
  }

  const waiting =
    capture && !pending ? `${capture.stop}:${capture.step}` : null;
  useEffect(() => {
    if (!waiting) return;
    return listen({
      pressed: (input) => void take(input),
      cancelled: () => end("stopped"),
      timedOut: () => end("stopped"),
      tick: (seconds) =>
        setCapture((current) => current && { ...current, seconds }),
      say,
    });
  }, [waiting]);

  return {
    capture,
    pending,
    /** Bind `members` in order, started from the button `stop`. */
    start(stop: string, members: string[]) {
      setPending(null);
      say("");
      setCapture({ stop, members, step: 0, seconds: CAPTURE_SECONDS });
    },
    cancel() {
      end("stopped");
    },
    /** Bind the key that another control uses to this one too. */
    useForBoth() {
      const current = latest.current.capture;
      if (!current || !pending) return;
      setKey(current.members[current.step], pending.key);
      setPending(null);
      advance();
    },
    /** Forget any capture, as when the pad changes. */
    reset() {
      setCapture(null);
      setPending(null);
    },
  };
}

function list(names: string[]): string {
  return names.length < 2
    ? names.join("")
    : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}
