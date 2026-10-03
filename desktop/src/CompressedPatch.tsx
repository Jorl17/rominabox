import { useEffect, useState } from "react";
import { Check } from "lucide-react";
import * as bridge from "./bridge";
import { formatBytes } from "./inspection";
import { leavingOut } from "./travelingFiles";

/** What we do with the files of the game on the details step. */
type FilesProps = {
  entry: string;
  system: string;
  files: bridge.GameFiles;
  onChange: (files: bridge.GameFiles, names: string[]) => void;
  onError: (error: unknown) => void;
};

const quoted = (names: string[]) => names.map((name) => `“${name}”`).join(", ");

/**
 * We can include a patch for a compressed disc (CHD) only if we decompress
 * the game, which makes it larger. So when a patch matches, we ask the author
 * and show the size either way. With Include patch we decompress the disc.
 * With Leave it out we leave the patch out, as in the Files list, and say so
 * in a second pop-up.
 */
export function CompressedPatchQuestion(props: FilesProps) {
  const { entry, system, files, onError } = props;
  const [asked, setAsked] = useState<bridge.Compressed | null>(null);
  const [leftOut, setLeftOut] = useState<string[] | null>(null);
  useEffect(() => {
    if (!entry || !system) return;
    let current = true;
    bridge
      .travelingFiles(entry, system, files)
      .then((listed) => {
        if (!current) return;
        const compressed = listed.compressed;
        setAsked(compressed && !compressed.included ? compressed : null);
      })
      .catch(onError);
    return () => {
      current = false;
    };
  }, [entry, system, files]);

  async function answer(next: bridge.GameFiles) {
    setAsked(null);
    try {
      const listed = await bridge.travelingFiles(entry, system, next);
      props.onChange(next, bridge.travelingNames(listed));
      return true;
    } catch (error) {
      onError(error);
      return false;
    }
  }
  async function leaveOut(patches: string[]) {
    if (await answer(leavingOut(files, patches))) setLeftOut(patches);
  }

  if (leftOut)
    return (
      <div className="pop-up-layer">
        <div
          className="pop-up pop-up-large"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="patch-left-out-title"
          aria-describedby="patch-left-out-body"
          onKeyDown={(event) => {
            if (event.key === "Escape") setLeftOut(null);
          }}
        >
          <h2 id="patch-left-out-title" className="pop-up-done">
            <Check size={22} strokeWidth={3} aria-hidden />
            Patch left out
          </h2>
          <p id="patch-left-out-body">
            {quoted(leftOut)} was left out. You can export the game as normal.
          </p>
          <div className="pop-up-actions">
            <button
              className="primary"
              onClick={() => setLeftOut(null)}
              autoFocus
            >
              OK
            </button>
          </div>
        </div>
      </div>
    );
  if (!asked) return null;
  return (
    <div className="pop-up-layer">
      <div
        className="pop-up pop-up-large"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="compressed-patch-title"
        aria-describedby="compressed-patch-body"
      >
        <h2 id="compressed-patch-title">Apply the patch?</h2>
        <p id="compressed-patch-body">
          {quoted(asked.patches)}{" "}
          {asked.patches.length === 1 ? "changes" : "change"} “{asked.game}”,
          which is compressed. To include the patch, the game will have to be
          decompressed and will result in a larger file. Do you really want to
          do this?
        </p>
        <p className="pop-up-sizes">
          <span>Without the patch: {formatBytes(asked.withoutBytes)}</span> ·{" "}
          <span>With the patch: {formatBytes(asked.withBytes)}</span>
        </p>
        <div className="pop-up-actions">
          <button
            className="secondary"
            onClick={() => void leaveOut(asked.patches)}
          >
            Leave it out
          </button>
          <button
            className="primary"
            onClick={() => void answer({ ...files, decompress: true })}
            autoFocus
          >
            Include patch
          </button>
        </div>
      </div>
    </div>
  );
}
