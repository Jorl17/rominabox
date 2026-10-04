import { useEffect, useState } from "react";
import {
  aboutComponents,
  appVersion,
  licenceText,
  openWebsite,
  type Component,
} from "./bridge";

/** The groups of the licence index, in the order of the dialog. */
const GROUPS: [Component["group"], string][] = [
  ["native", "Game player"],
  ["cores", "Emulator cores"],
  ["data", "Data"],
  ["fonts", "Fonts"],
  ["crates", "Rust crates"],
  ["npm", "Interface packages"],
  ["toolchains", "Toolchains"],
];

/** One component. We read its licence text when someone opens it. */
function Entry({ component }: { component: Component }) {
  const [text, setText] = useState<string | null>(null);
  return (
    <details
      className="about-entry"
      onToggle={(event) => {
        if (event.currentTarget.open && text === null)
          licenceText(component.file).then(setText, (error) =>
            setText(String(error)),
          );
      }}
    >
      <summary>
        <span className="about-title">{component.title}</span>
        <span className="about-licence">{component.licence}</span>
      </summary>
      <pre className="about-text">{text ?? "Loading…"}</pre>
    </details>
  );
}

/**
 * The components of the builder and of the games made with it, each with its
 * licence, from the licence index bundled with the builder.
 */
export function AboutDialog({ onClose }: { onClose: () => void }) {
  const [components, setComponents] = useState<Component[] | null>(null);
  const [version, setVersion] = useState("");
  useEffect(() => {
    aboutComponents().then(setComponents, () => setComponents([]));
    appVersion().then(setVersion, () => {});
  }, []);
  return (
    <div className="pop-up-layer" onClick={onClose}>
      <div
        className="pop-up pop-up-about"
        role="dialog"
        aria-modal="true"
        aria-labelledby="about-heading"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
        }}
      >
        <h2 id="about-heading">ROM-in-a-Box {version}</h2>
        {(components ?? [])
          .filter((component) => component.copyright)
          .map((component) => (
            <p className="about-copyright" key={component.file}>
              {component.copyright}, {component.licence}
            </p>
          ))}
        <button
          className="text-button about-website"
          onClick={() => openWebsite()}
        >
          {__WEBSITE__.replace(/^https?:\/\//, "")}
        </button>
        <p>Made from these components. Open one to read its licence.</p>
        <div className="about-list">
          {components === null && <p>Loading…</p>}
          {GROUPS.map(([group, heading]) => {
            const members = (components ?? []).filter(
              (component) => component.group === group,
            );
            return (
              members.length > 0 && (
                <section key={group}>
                  <h3>{heading}</h3>
                  {members.map((component) => (
                    <Entry key={component.file} component={component} />
                  ))}
                </section>
              )
            );
          })}
        </div>
        <div className="pop-up-actions">
          <button className="primary" onClick={onClose} autoFocus>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
