import type { Draft } from "./App";
import { ChevronRight } from "lucide-react";
import { ControlsEditor, type Controls } from "./controls";
import { HotkeysEditor } from "./HotkeysEditor";
import { hotkeyIds, type Hotkeys } from "./hotkeys";
import { fastForwardHotkey } from "./fastForward";

type Settings = Pick<
  Draft,
  "system" | "showMenu" | "hotkeys" | "fastForward" | "fastForwardHold"
>;

/**
 * The Controls part of the menu step: the game's controls, and its hotkeys
 * when it has a menu. We check each against the other with the export's
 * rules: a hotkey that acts during play may not use any input of the game.
 * The Fast forward hotkey is there only when the game has fast forward,
 * with the choice between running while held and from one press to the next.
 */
export function ControlsSection({
  settings,
  update,
  controls,
  onControls,
  busy,
}: {
  settings: Settings;
  update: <K extends keyof Draft>(key: K, value: Draft[K]) => void;
  controls: Controls;
  onControls: (controls: Controls) => void;
  busy: boolean;
}) {
  const { system, hotkeys } = settings;
  return (
    <details className="advanced author-controls">
      <summary>
        <ChevronRight size={16} />
        Controls
      </summary>
      <ControlsEditor
        system={system}
        value={controls}
        onChange={onControls}
        hotkeys={hotkeys}
      />
      {settings.showMenu && (
        <HotkeysEditor
          value={hotkeys}
          busy={busy}
          onChange={(value) => update("hotkeys", value)}
          game={{ system, controls }}
          rows={hotkeyIds.filter(
            (hotkey) => hotkey !== fastForwardHotkey || settings.fastForward,
          )}
          extra={(hotkey) =>
            hotkey === fastForwardHotkey && (
              <select
                className="hotkey-mode"
                aria-label="Fast forward runs"
                value={settings.fastForwardHold ? "hold" : "toggle"}
                disabled={busy}
                onChange={(e) =>
                  update("fastForwardHold", e.target.value === "hold")
                }
              >
                <option value="hold">Hold</option>
                <option value="toggle">Toggle</option>
              </select>
            )
          }
        />
      )}
    </details>
  );
}
