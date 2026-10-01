import { ChevronRight } from "lucide-react";
import { ControlsEditor, type Controls } from "./controls";
import { HotkeysEditor } from "./HotkeysEditor";
import type { Hotkeys } from "./hotkeys";

/**
 * Controls on the menu step: the game's controls, and its hotkeys when the
 * game has a menu. We check each against the other with the export rules: a
 * hotkey that works while the game runs must not use any input of the
 * game.
 */
export function ControlsSection({
  system,
  controls,
  onControls,
  hotkeys,
  onHotkeys,
  withHotkeys,
  busy,
}: {
  system: string;
  controls: Controls;
  onControls: (controls: Controls) => void;
  hotkeys: Hotkeys;
  onHotkeys: (hotkeys: Hotkeys) => void;
  withHotkeys: boolean;
  busy: boolean;
}) {
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
      {withHotkeys && (
        <HotkeysEditor
          value={hotkeys}
          busy={busy}
          onChange={onHotkeys}
          game={{ system, controls }}
        />
      )}
    </details>
  );
}
