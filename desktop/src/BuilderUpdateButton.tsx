import { useEffect, useState } from "react";
import {
  checkBuilderUpdate,
  installBuilderUpdate,
  openWebsite,
  type BuilderUpdate,
} from "./bridge";
import { checksForUpdates } from "./updatePreference";

/** "Update to x.y.z" in the header when there is a newer builder. We look
 * once, when the builder starts, unless the person turned it off in About,
 * and show nothing when we find nothing or cannot look. Pressing it installs
 * the update and starts the new builder, or opens the website when we cannot
 * replace the builder where it is. */
export function BuilderUpdateButton() {
  const [update, setUpdate] = useState<BuilderUpdate | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    if (!checksForUpdates()) return;
    checkBuilderUpdate().then(setUpdate, () => {});
  }, []);

  if (!update) return null;

  function install() {
    if (!update?.installable) {
      void openWebsite();
      return;
    }
    setFailure(null);
    setProgress("Updating…");
    installBuilderUpdate((received, total) =>
      setProgress(
        total
          ? `Updating… ${Math.floor((received / total) * 100)}%`
          : "Updating…",
      ),
    ).catch((reason) => {
      setProgress(null);
      setFailure(String(reason));
    });
  }

  return (
    <>
      {failure && <span className="builder-update-failure">{failure}</span>}
      <button
        className="text-button header-link builder-update"
        disabled={progress !== null}
        title={update.notes ?? undefined}
        onClick={install}
      >
        {progress ?? `Update to ${update.version}`}
      </button>
    </>
  );
}
