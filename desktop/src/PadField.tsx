import { useEffect, useRef, useState } from "react";
import { cancelPadCapture, capturePadPosition } from "./bridge";
import { moveTo, padPositions, positionName, positionOf } from "./padPositions";

const SECONDS = 10;

type Control = { id: string; label: string };

/** The pad position a control is read from: chosen, or pressed on a controller. */
export function PadField({
  control,
  bindings,
  movable,
  fixed,
  disabled,
  onMove,
  onMessage,
}: {
  control: Control;
  bindings: Record<string, { pad?: string } | undefined>;
  movable: Control[];
  fixed: Control[];
  disabled: boolean;
  onMove: (moved: Record<string, string | undefined>) => void;
  onMessage: (message: string) => void;
}) {
  const [waiting, setWaiting] = useState(false);
  const [seconds, setSeconds] = useState(SECONDS);
  const held = new Set(fixed.map((other) => other.id));
  function choose(to: string) {
    const moved = moveTo(movable, fixed, bindings, control.id, to);
    if (!moved) {
      onMessage(`${positionName(to)} is taken.`);
      return;
    }
    onMove(moved);
    onMessage("Pad updated.");
  }
  // The answer arrives after renders, so we apply it to the controls as they
  // are then.
  const latest = useRef(choose);
  latest.current = choose;
  const message = useRef(onMessage);
  message.current = onMessage;

  useEffect(() => {
    if (!waiting) return;
    let live = true;
    const deadline = performance.now() + SECONDS * 1000;
    const timer = window.setInterval(
      () =>
        setSeconds(
          Math.max(0, Math.ceil((deadline - performance.now()) / 1000)),
        ),
      100,
    );
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      void cancelPadCapture();
    };
    window.addEventListener("keydown", escape, true);
    capturePadPosition(SECONDS)
      .then((pressed) => {
        if (!live) return;
        if (pressed) latest.current(pressed);
        else message.current("Binding unchanged.");
      })
      .catch((error: unknown) => {
        if (live)
          message.current(
            error instanceof Error ? error.message : String(error),
          );
      })
      .finally(() => {
        if (live) setWaiting(false);
      });
    return () => {
      live = false;
      clearInterval(timer);
      window.removeEventListener("keydown", escape, true);
      void cancelPadCapture();
    };
  }, [waiting]);

  return (
    <div className="pad-field">
      <label>
        Pad
        <select
          aria-label={`${control.label} pad`}
          value={positionOf(control, bindings)}
          disabled={disabled || waiting}
          onChange={(e) => choose(e.target.value)}
        >
          {padPositions.map((position) => (
            <option
              key={position.id}
              value={position.id}
              disabled={held.has(position.id)}
            >
              {position.name}
            </option>
          ))}
        </select>
      </label>
      {waiting ? (
        <>
          <span role="status">Press a button… {seconds}</span>
          <button
            type="button"
            className="secondary"
            onClick={() => void cancelPadCapture()}
          >
            Cancel
          </button>
        </>
      ) : (
        <button
          type="button"
          className="secondary"
          disabled={disabled}
          onClick={() => {
            setSeconds(SECONDS);
            setWaiting(true);
          }}
        >
          Press on controller
        </button>
      )}
    </div>
  );
}
