import { useEffect, useRef, useState } from "react";
import { Plus, X } from "lucide-react";
import * as bridge from "./bridge";
import { CAPTURE_SECONDS, listen } from "./bindingCapture";
import {
  actionName,
  bindingOf,
  bindingWords,
  defaultMenuControls,
  menuActions,
  refusalWords,
  type MenuAction,
  type MenuControls,
} from "./menuControls";

/**
 * The inputs to open the game's menu, and confirm and go back in it, until the
 * player changes them on MENU CONTROLS. A press is a key or a pad button, as
 * in the capture in the game, and we apply the export rules.
 */
export function MenuControlsEditor({
  value,
  onChange,
  busy,
}: {
  value: MenuControls;
  onChange: (next: MenuControls) => void;
  busy: boolean;
}) {
  const [waiting, setWaiting] = useState<{
    action: MenuAction;
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
  async function propose(next: MenuControls, changed: MenuAction) {
    const refusal = await bridge.checkMenuControls(next);
    setSaid(refusal ? refusalWords(refusal, changed) : "");
    if (!refusal) onChange(next);
  }
  function add(action: MenuAction) {
    end();
    setSaid("");
    setWaiting({ action, seconds: CAPTURE_SECONDS });
    stop.current = listen({
      pressed: (input) => {
        end();
        const binding = bindingOf(input);
        if (!binding) setSaid("Use a key or a pad button.");
        else if (!value[action].includes(binding))
          void propose(
            { ...value, [action]: [...value[action], binding] },
            action,
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
    <div className="menu-controls">
      <div className="controls-toolbar">
        <strong>Menu controls</strong>
        <button
          type="button"
          className="text-button"
          disabled={busy}
          onClick={() => {
            end();
            setSaid("");
            onChange(defaultMenuControls);
          }}
        >
          Reset to defaults
        </button>
      </div>
      {menuActions.map((action) => (
        <div className="menu-control-row" key={action}>
          <span className="menu-control-name">{actionName(action)}</span>
          {value[action].map((binding) => (
            <span className="menu-control-chip" key={binding}>
              {bindingWords(binding)}
              <button
                type="button"
                className="menu-control-remove"
                disabled={busy}
                aria-label={`Remove ${bindingWords(binding)} from ${actionName(action)}`}
                onClick={() =>
                  void propose(
                    {
                      ...value,
                      [action]: value[action].filter((one) => one !== binding),
                    },
                    action,
                  )
                }
              >
                <X size={12} aria-hidden="true" />
              </button>
            </span>
          ))}
          {waiting?.action === action ? (
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
              aria-label={`Add to ${actionName(action)}`}
              onClick={() => add(action)}
            >
              <Plus size={14} aria-hidden="true" />
            </button>
          )}
        </div>
      ))}
      <span className="control-message" role="status">
        {said}
      </span>
    </div>
  );
}
