import { FolderOpen, MonitorSmartphone } from "lucide-react";
import { useState } from "react";
import * as bridge from "./bridge";
import { Checkbox, Help } from "./Help";

/** The name of an export, for its platform. */
export function exportProduct(target: bridge.ExportTarget | null): string {
  switch (target) {
    case "macos":
      return "MACOS APP";
    case "windows":
      return "WINDOWS APP";
    case null:
      return "";
  }
}

/** The name a person uses for a platform: this machine as "This Mac" or
 * "This PC", the other one by its name. */
function platformName(
  target: bridge.ExportTarget,
  host: bridge.ExportTarget | null,
): string {
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
  target: bridge.ExportTarget | null;
  onTarget: (target: bridge.ExportTarget) => void;
  destination: string;
  onDestination: (folder: string) => void;
  intelMacs: boolean;
  onIntelMacs: (value: boolean) => void;
  fail: (reason: unknown) => void;
}) {
  const [choosing, setChoosing] = useState(false);
  // This machine's platform first, then the other.
  const platforms: bridge.ExportTarget[] =
    host === "windows" ? ["windows", "macos"] : ["macos", "windows"];
  function choose(chosen: bridge.ExportTarget) {
    setChoosing(false);
    if (chosen === target) return;
    onTarget(chosen);
    // A Mac game made elsewhere is for whichever Macs its players have.
    if (chosen === "macos" && host !== "macos") onIntelMacs(true);
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
      {target === "macos" && (
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
