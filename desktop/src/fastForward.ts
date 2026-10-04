import settings from "../../vendor/retroarch/menu/drivers/rmlui/settings.inc?raw";
import type { Hotkey } from "./hotkeys";

/** The hotkey that a game has only with fast forward on. */
export const fastForwardHotkey: Hotkey = "fast-forward";

/** The fast forward speeds, the positions declared for it in settings.inc,
 * as values of the RetroArch fastforward_ratio, where 0 means as fast as the
 * computer can run. */
export const fastForwardSpeeds: number[] = (() => {
  const found = settings.match(
    /^RIB_SETTING_POSITIONS\(FastforwardRatio, "([^"]*)"\)$/m,
  );
  if (!found) throw new Error("settings.inc declares no fast forward speeds");
  return found[1].split(" ").map(Number);
})();

/** A speed in the builder's words: "2×", or "Max" for the fastest. */
export function speedWords(speed: number): string {
  return speed === 0 ? "Max" : `${speed}×`;
}
