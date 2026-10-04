import type { Draft } from "./App";
import { ChevronRight } from "lucide-react";
import { Checkbox } from "./Help";
import { fastForwardSpeeds, speedWords } from "./fastForward";

type Settings = Pick<
  Draft,
  | "everyPadIsPlayerOne"
  | "advancedEmulatorAccess"
  | "fastForward"
  | "fastForwardSpeed"
>;

/** The Advanced section of the Menu step: choices most authors leave alone. */
export function AdvancedOptions({
  settings,
  update,
}: {
  settings: Settings;
  update: <K extends keyof Draft>(key: K, value: Draft[K]) => void;
}) {
  return (
    <details className="advanced">
      <summary>
        <ChevronRight size={16} />
        Advanced
      </summary>
      <Checkbox
        label="Every controller is player 1"
        checked={settings.everyPadIsPlayerOne}
        onChange={(value) => update("everyPadIsPlayerOne", value)}
        help="Turn off for games with a second player."
      />
      <Checkbox
        label="Advanced emulator access"
        checked={settings.advancedEmulatorAccess}
        onChange={(value) => update("advancedEmulatorAccess", value)}
        help="Restore RetroArch's native menus."
      />
      <Checkbox
        label="Fast forward"
        checked={settings.fastForward}
        onChange={(value) => update("fastForward", value)}
        help="A hotkey runs the game faster."
      />
      {settings.fastForward && (
        <div className="labeled-choice fast-forward-speed">
          <label htmlFor="fast-forward-speed">Speed</label>
          <select
            id="fast-forward-speed"
            value={String(settings.fastForwardSpeed)}
            onChange={(e) => update("fastForwardSpeed", Number(e.target.value))}
          >
            {fastForwardSpeeds.map((speed) => (
              <option value={String(speed)} key={speed}>
                {speedWords(speed)}
              </option>
            ))}
          </select>
        </div>
      )}
    </details>
  );
}
