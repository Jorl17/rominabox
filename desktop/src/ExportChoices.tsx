import { FolderOpen, MonitorSmartphone } from "lucide-react";
import { useState } from "react";
import * as bridge from "./bridge";
import { Checkbox, Help } from "./Help";

/** What a game can be made for: one platform, or both in one zip. */
export type Platform = bridge.ExportTarget | "both";

/** The name of an export, for its target. */
export function exportProduct(target: Platform | null): string {
  switch (target) {
    case "macos":
      return "MACOS APP";
    case "windows":
      return "WINDOWS APP";
    case "both":
      return "MAC AND WINDOWS ZIP";
    case null:
      return "";
  }
}

/** The name a person uses for a platform: this machine as "This Mac" or
 * "This PC", the other one by its name. */
function platformName(
  target: Platform,
  host: bridge.ExportTarget | null,
): string {
  if (target === "both") return "Mac and Windows";
  if (target === host) return target === "macos" ? "This Mac" : "This PC";
  return target === "macos" ? "Mac" : "Windows";
}

/** What we ask on the export step before we make the app: where it goes,
 * which platform it is for, and, for a Mac app, whether it also runs on Intel
 * Macs. The platform is that of this machine until the author opens Change,
 * because a game for another platform is a deliberate choice, and we never
 * ask for it along the steps. */
export function ExportChoices({
  host,
  target,
  onTarget,
  destination,
  onDestination,
  intelMacs,
  onIntelMacs,
  fail,
}: {
  host: bridge.ExportTarget | null;
  target: Platform | null;
  onTarget: (target: Platform) => void;
  destination: string;
  onDestination: (folder: string) => void;
  intelMacs: boolean;
  onIntelMacs: (value: boolean) => void;
  fail: (reason: unknown) => void;
}) {
  const [choosing, setChoosing] = useState(false);
  // This machine's platform first, then the other, then both in one zip.
  const platforms: Platform[] =
    host === "windows"
      ? ["windows", "macos", "both"]
      : ["macos", "windows", "both"];
  function choose(chosen: Platform) {
    setChoosing(false);
    if (chosen === target) return;
    onTarget(chosen);
    // A Mac game made elsewhere, or shared with everyone in one zip, is for
    // whichever Macs its players have.
    if (chosen === "both" || (chosen === "macos" && host !== "macos"))
      onIntelMacs(true);
  }
  return (
    <>
      <div className="destination">
        <label>
          Save to
          <Help>An existing app will not be silently replaced.</Help>
        </label>
        <button
          onClick={async () => {
            if (!bridge.native) return;
            try {
              const folder = await bridge.pickFolder();
              if (folder) onDestination(folder);
            } catch (e) {
              fail(e);
            }
          }}
        >
          <FolderOpen size={19} />
          <span>{destination || "Downloads / ROM-in-a-Box"}</span>
          <span>Change</span>
        </button>
      </div>
      {host && target && (
        <div className="destination platform">
          <label>
            Made for
            <Help>
              A game runs on the kind of computer it is made for. Change it to
              make this game for someone on a different one.
            </Help>
          </label>
          <button
            aria-expanded={choosing}
            onClick={() => setChoosing((open) => !open)}
          >
            <MonitorSmartphone size={19} />
            <span>{platformName(target, host)}</span>
            <span>Change</span>
          </button>
          {choosing && (
            <div className="platform-choices" role="radiogroup">
              {platforms.map((platform) => (
                <label key={platform}>
                  <input
                    type="radio"
                    name="platform"
                    checked={platform === target}
                    onChange={() => choose(platform)}
                  />
                  {platformName(platform, host)}
                </label>
              ))}
            </div>
          )}
        </div>
      )}
      {(target === "macos" || target === "both") && (
        <Checkbox
          className="export-choice"
          label="Also runs on Intel Macs"
          checked={intelMacs}
          onChange={onIntelMacs}
          help="Include what Intel Macs need too, which makes the app about twice as large."
        />
      )}
    </>
  );
}
