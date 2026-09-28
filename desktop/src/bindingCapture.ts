import { cancelPadCapture, capturePadPosition } from "./bridge";
import { capturedKey } from "./keys";

/** How long we wait for an input for one control, as in the game's capture. */
export const CAPTURE_SECONDS = 10;

/** The input for a control: a key by its stored name, or a pad position. */
export type Pressed =
  { kind: "key"; key: string } | { kind: "pad"; position: string };

/**
 * One wait for the input of one control, from the keyboard and every
 * connected controller at once. As in the game's capture, we take whatever
 * the player presses first, but not the key that started the wait. Escape,
 * leaving the window or the deadline ends the wait.
 */
export type Listening = {
  pressed: (input: Pressed) => void;
  cancelled: () => void;
  timedOut: () => void;
  tick: (seconds: number) => void;
  /** A message for the author while the wait goes on. */
  say: (message: string) => void;
};

// We end the wait for a controller before we request the next one, so we
// never mistake the end of one for the end of the other.
let ended: Promise<void> = Promise.resolve();

/** Start waiting, and return a function that stops the wait. */
export function listen(to: Listening): () => void {
  let live = true;
  const deadline = performance.now() + CAPTURE_SECONDS * 1000;
  function stop() {
    if (!live) return;
    live = false;
    window.clearInterval(timer);
    window.removeEventListener("keydown", key, true);
    window.removeEventListener("blur", blur);
    ended = cancelPadCapture();
  }
  function finish(then: () => void) {
    if (!live) return;
    stop();
    then();
  }
  const timer = window.setInterval(() => {
    const remaining = Math.max(
      0,
      Math.ceil((deadline - performance.now()) / 1000),
    );
    to.tick(remaining);
    if (!remaining) finish(to.timedOut);
  }, 100);
  function key(event: KeyboardEvent) {
    event.preventDefault();
    event.stopImmediatePropagation();
    if (event.repeat) return;
    if (event.key === "Escape") {
      finish(to.cancelled);
      return;
    }
    const name = capturedKey(event);
    if (!name) {
      to.say("Choose a letter, number, arrow or modifier key.");
      return;
    }
    finish(() => to.pressed({ kind: "key", key: name }));
  }
  const blur = () => finish(to.cancelled);
  window.addEventListener("keydown", key, true);
  window.addEventListener("blur", blur);
  ended
    .then(() => (live ? capturePadPosition(CAPTURE_SECONDS) : null))
    .then((position) => {
      if (live && position) finish(() => to.pressed({ kind: "pad", position }));
    })
    .catch((error: unknown) => {
      // We still read the keyboard when we cannot read any controller.
      if (live) to.say(error instanceof Error ? error.message : String(error));
    });
  return stop;
}
