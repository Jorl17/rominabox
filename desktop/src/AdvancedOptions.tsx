import { ChevronRight } from "lucide-react";
import { Checkbox } from "./Help";

type Settings = {
  everyPadIsPlayerOne: boolean;
  advancedEmulatorAccess: boolean;
};

/** The Advanced section of the Menu step: choices most authors leave alone. */
export function AdvancedOptions({
  settings,
  update,
}: {
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
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
        help="Restore RetroArch's native menus. Ordinary exports keep About, Hide, Quit and standard window actions."
      />
    </details>
  );
}
