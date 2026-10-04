import * as bridge from "./bridge";

/** The error on screen. For a failure that comes from a bug, we ask the
 * author to open an issue, with the details folded away. */
export function ErrorNotice({
  error,
  bug,
}: {
  error: string;
  bug: bridge.ExportBug | null;
}) {
  if (!error) return null;
  return (
    <div className="error" role="alert">
      {error}
      {bug?.message === error && (
        <>
          {" "}
          <button
            type="button"
            className="link"
            onClick={() => void bridge.openBugReport()}
          >
            Please open an issue.
          </button>
          <details className="bug-details">
            <summary>Details</summary>
            <code>{bug.details}</code>
          </details>
        </>
      )}
    </div>
  );
}
