import {
  CoreDownloadFailed,
  type CoreActivity,
  type ExportProgress,
} from "./bridge";

/**
 * The one pop-up about cores on the export step.
 *
 * It appears only when we download or update a core in the export, and closes
 * when the export ends. When a required core is not cached and we cannot
 * download it, the pop-up stays open with Go back and Retry.
 */
export type CoreNotice =
  | Extract<CoreActivity, { kind: "fetching" }>
  | { kind: "failed"; message: string };

/** Open the pop-up on a progress event about fetching cores. */
export function afterProgress(
  current: CoreNotice | null,
  progress: ExportProgress,
): CoreNotice | null {
  return progress.cores?.kind === "fetching" ? progress.cores : current;
}

/** The export ended, with `failure` or without. Close the pop-up unless we
 * could not download a core. */
export function afterExport(failure?: unknown): CoreNotice | null {
  return failure instanceof CoreDownloadFailed
    ? { kind: "failed", message: failure.message }
    : null;
}

function cores(count: number) {
  return `${count} core${count === 1 ? "" : "s"}`;
}

export function fetchingLines(downloading: number, updating: number) {
  return [
    ...(downloading > 0 ? [`Downloading ${cores(downloading)}`] : []),
    ...(updating > 0 ? [`Updating ${cores(updating)}`] : []),
  ];
}

export function CoreFetchNotice({
  notice,
  onBack,
  onRetry,
}: {
  notice: CoreNotice;
  onBack: () => void;
  onRetry: () => void;
}) {
  if (notice.kind === "fetching")
    return (
      <div className="core-fetch-layer">
        <div className="core-fetch" role="status">
          {fetchingLines(notice.downloading, notice.updating).map((line) => (
            <p key={line}>{line}</p>
          ))}
        </div>
      </div>
    );
  return (
    <div className="core-fetch-layer">
      <div
        className="core-fetch"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="core-fetch-failed"
      >
        <p id="core-fetch-failed">{notice.message}</p>
        <div className="core-fetch-actions">
          <button className="secondary" onClick={onBack}>
            Go back
          </button>
          <button className="primary" onClick={onRetry} autoFocus>
            Retry
          </button>
        </div>
      </div>
    </div>
  );
}
