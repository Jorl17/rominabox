import { useEffect, useRef, useState, type ReactNode } from "react";
import { Plus, X } from "lucide-react";
import * as bridge from "./bridge";
import { CAPTURE_SECONDS, listen } from "./bindingCapture";
import type { Controls } from "./controls";
import {
  bindingOf,
  bindingWords,
  defaultHotkeys,
  hotkeyIds,
  hotkeyName,
  refusalWords,
  type Hotkey,
  type Hotkeys,
} from "./hotkeys";

/**
 * The game's hotkeys until the player changes them on HOTKEYS: the inputs to
 * open the menu, confirm and go back in it, and to save, load and change the
 * slot while the game runs. A press is a key or a pad button, as in the
 * capture in the game. We apply the export rules, with the controls of
 * `game`, whose keys a hotkey that works while the game runs must not use.
 */
export function HotkeysEditor({
  value,
  onChange,
  busy,
  game,
  rows = hotkeyIds,
  extra,
}: {
  value: Hotkeys;
  onChange: (next: Hotkeys) => void;
  busy: boolean;
  game: { system: string; controls: Controls };
  /** The hotkeys the game has, in the order the game declares them. */
  rows?: Hotkey[];
  /** A control at the end of a row, after its bindings. */
  extra?: (hotkey: Hotkey) => ReactNode;
}) {
  const [waiting, setWaiting] = useState<{
    hotkey: Hotkey;
    seconds: number;
  } | null>(null);
  const [said, setSaid] = useState("");
  const stop = useRef<(() => void) | null>(null);
  useEffect(() => () => stop.current?.(), []);

  function end() {
    stop.current?.();
    stop.current = null;
    setWaiting(null);
  }
  async function propose(next: Hotkeys, changed: Hotkey) {
    // We check a hotkey that the game does not have as bound to nothing, and
    // keep its bindings for when the game has it again.
    const offered = Object.fromEntries(
      hotkeyIds.map((hotkey) => [
        hotkey,
        rows.includes(hotkey) ? next[hotkey] : [],
      ]),
    ) as Hotkeys;
    const refusal = await bridge.checkHotkeys(
      offered,
      game.system,
      game.controls,
    );
    setSaid(refusal ? refusalWords(refusal, changed) : "");
    if (!refusal) onChange(next);
  }
  function add(hotkey: Hotkey) {
    end();
    setSaid("");
    setWaiting({ hotkey, seconds: CAPTURE_SECONDS });
    stop.current = listen({
      pressed: (input) => {
        end();
        const binding = bindingOf(input);
        if (!binding) setSaid("Use a key or a pad button.");
        else if (!value[hotkey].includes(binding))
          void propose(
            { ...value, [hotkey]: [...value[hotkey], binding] },
            hotkey,
          );
      },
      cancelled: end,
      timedOut: end,
      tick: (seconds) =>
        setWaiting((current) => current && { ...current, seconds }),
      say: setSaid,
    });
  }

  return (
    <div className="hotkeys">
      <div className="controls-toolbar">
        <strong>Hotkeys</strong>
        <button
          type="button"
          className="text-button"
          disabled={busy}
          onClick={() => {
            end();
            setSaid("");
            onChange(defaultHotkeys);
          }}
        >
          Reset to defaults
        </button>
      </div>
      {rows.map((hotkey) => (
        <div className="hotkey-row" key={hotkey}>
          <span className="hotkey-name">{hotkeyName(hotkey)}</span>
          {value[hotkey].map((binding) => (
            <span className="hotkey-chip" key={binding}>
              {bindingWords(binding)}
              <button
                type="button"
                className="hotkey-remove"
                disabled={busy}
                aria-label={`Remove ${bindingWords(binding)} from ${hotkeyName(hotkey)}`}
                onClick={() =>
                  void propose(
                    {
                      ...value,
                      [hotkey]: value[hotkey].filter((one) => one !== binding),
                    },
                    hotkey,
                  )
                }
              >
                <X size={12} aria-hidden="true" />
              </button>
            </span>
          ))}
          {waiting?.hotkey === hotkey ? (
            <>
              <span className="binding-text">
                Press an input… {waiting.seconds}
              </span>
              <button type="button" className="secondary" onClick={end}>
                Cancel
              </button>
            </>
          ) : (
            <button
              type="button"
              className="secondary"
              disabled={busy || !!waiting}
              aria-label={`Add to ${hotkeyName(hotkey)}`}
              onClick={() => add(hotkey)}
            >
              <Plus size={14} aria-hidden="true" />
            </button>
          )}
          {extra?.(hotkey)}
        </div>
      ))}
      <span className="control-message" role="status">
        {said}
      </span>
    </div>
  );
}
