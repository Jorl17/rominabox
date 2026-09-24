import { AppExists } from "./bridge";

/**
 * We ask this on Create app when an app with this name is already in the
 * folder, before we do anything. With Cancel the author returns to the export
 * step, and with Replace we export again and replace the app.
 */
export function ReplaceAppDialog({
  existing,
  onCancel,
  onReplace,
}: {
  existing: AppExists;
  onCancel: () => void;
  onReplace: () => void;
}) {
  return (
    <div className="pop-up-layer">
      <div
        className="pop-up"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="replace-app-title"
        aria-describedby="replace-app-body"
        onKeyDown={(event) => {
          if (event.key === "Escape") onCancel();
        }}
      >
        <h2 id="replace-app-title">Replace “{existing.app}”?</h2>
        <p id="replace-app-body">
          An app with this name already exists in {existing.folder}.
        </p>
        <div className="pop-up-actions">
          <button className="secondary" onClick={onCancel} autoFocus>
            Cancel
          </button>
          <button className="primary" onClick={onReplace}>
            Replace
          </button>
        </div>
      </div>
    </div>
  );
}
