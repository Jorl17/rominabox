import { beforeExport, ExportRequest, NoticeSection } from "./bridge";
import { ShaderWarning } from "./ShaderWarning";

/** What we ask about on Create app: the shaders that will not load on the
 * platforms of the export, and an app with this name already in the folder. */
export type Asking = {
  notice: NoticeSection[];
  existing: { app: string; folder: string } | null;
};

/** On Create app, `ask` what there is to ask before exporting `request`, or
 * `create` the app at once when there is nothing. */
export async function askFirst(
  request: ExportRequest,
  ask: (asking: Asking) => void,
  create: () => void,
) {
  const asked = await beforeExport(request);
  if (asked.notice.length > 0 || asked.existing) ask(asked);
  else create();
}

/**
 * The one pop-up on Create app, before we make anything, when there is
 * something to ask. With only an app in the way, we ask whether to replace
 * it. With shaders that will not load, we list them, by platform when more
 * than one platform has some, and say last that we will replace the app in
 * the way. With Cancel the author returns to the export step, and with the
 * other button we export, replacing the app in the way.
 */
export function CreateAppDialog({
  title,
  asking: { notice, existing },
  close,
  create,
}: {
  title: string;
  asking: Asking;
  close: () => void;
  create: (replace: boolean) => void;
}) {
  const replacing = notice.length === 0 && existing;
  return (
    <div className="pop-up-layer">
      <div
        className={`pop-up ${replacing ? "" : "pop-up-large"}`}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="create-app-title"
        aria-describedby="create-app-body"
        onKeyDown={(event) => {
          if (event.key === "Escape") close();
        }}
      >
        <h2 id="create-app-title">
          {replacing
            ? `Replace “${replacing.app}”?`
            : `Before we create “${title}”`}
        </h2>
        <div id="create-app-body">
          {notice.map((section) => (
            <section className="pop-up-section" key={section.heading ?? ""}>
              {section.heading && <h3>{section.heading}</h3>}
              {section.warnings.map((warning) => (
                <ShaderWarning key={warning.text} warning={warning} />
              ))}
            </section>
          ))}
          {existing && (
            <p className="pop-up-section">
              An app with this name already exists in {existing.folder}.
              {replacing ? "" : " We’ll replace it."}
            </p>
          )}
        </div>
        <div className="pop-up-actions">
          <button className="secondary" onClick={close} autoFocus>
            Cancel
          </button>
          <button
            className="primary"
            onClick={() => {
              close();
              create(existing !== null);
            }}
          >
            {replacing ? "Replace" : "Create anyway"}
          </button>
        </div>
      </div>
    </div>
  );
}
