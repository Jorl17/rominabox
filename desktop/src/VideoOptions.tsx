import type { ReactNode } from "react";
import settingsInc from "../../vendor/retroarch/menu/drivers/rmlui/settings.inc?raw";
import type { Draft } from "./App";
import { Checkbox } from "./Help";
import "./VideoOptions.css";

/** The values at the positions of a level, as we declare them for the
 * player in settings.inc. */
function positions(name: string): number[] {
  const found = settingsInc.match(
    new RegExp(`RIB_SETTING_POSITIONS\\(${name}, "([^"]*)"\\)`),
  );
  if (!found) throw new Error(`settings.inc declares no positions for ${name}`);
  return found[1].trim().split(/\s+/).map(Number);
}

const levels = [
  {
    key: "brightness",
    label: "Brightness",
    values: positions("VideoBrightness"),
  },
  { key: "contrast", label: "Contrast", values: positions("VideoContrast") },
] as const;

/** The position whose value is nearest `value`. */
function nearest(values: readonly number[], value: number): number {
  return values.reduce(
    (best, each, index) =>
      Math.abs(each - value) < Math.abs(values[best] - value) ? index : best,
    0,
  );
}

/** VIDEO in the game's Options, with the tick for it, the light and the
 * contrast of the picture at the start of the game, and, inside it, the
 * shaders the player can choose there (`children`). */
export function VideoOptions({
  draft,
  update,
  children,
}: {
  draft: Draft;
  update: <K extends keyof Draft>(key: K, value: Draft[K]) => void;
  children: ReactNode;
}) {
  const on = draft.showMenu && draft.video;
  return (
    <div className="video-options">
      <Checkbox
        label="Video options"
        checked={on}
        disabled={!draft.showMenu}
        onChange={(value) => update("video", value)}
        help="Let the player adjust brightness and contrast."
      />
      {on && (
        <div className="video-settings">
          {levels.map(({ key, label, values }) => {
            const at = nearest(values, draft[key]);
            const percent = `${Math.round(values[at] * 100)}%`;
            return (
              <label className="video-level" key={key}>
                <span>{label}</span>
                <input
                  type="range"
                  min={0}
                  max={values.length - 1}
                  step={1}
                  value={at}
                  aria-valuetext={percent}
                  onChange={(event) =>
                    update(key, values[Number(event.target.value)])
                  }
                />
                <output>{percent}</output>
              </label>
            );
          })}
          {children}
        </div>
      )}
    </div>
  );
}
