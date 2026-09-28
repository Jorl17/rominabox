import { FolderOpen } from "lucide-react";
import * as bridge from "./bridge";
import { Checkbox, Help } from "./Help";

/** What we ask on the export step before we make the app: where it goes,
 * and, for a Mac app, whether it also runs on Intel Macs. */
export function ExportChoices({
  target,
  destination,
  onDestination,
  intelMacs,
  onIntelMacs,
  fail,
}: {
  target: bridge.ExportTarget | null;
  destination: string;
  onDestination: (folder: string) => void;
  intelMacs: boolean;
  onIntelMacs: (value: boolean) => void;
  fail: (reason: unknown) => void;
}) {
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
