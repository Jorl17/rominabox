import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import registry from "../controls.json";
import systemRegistry from "../systems.json";
import { ControlsEditor, emptyControls, type Controls } from "./controls";
import { defaultHotkeys } from "./hotkeys";
import "./style.css";

// Every console with a dedicated pad, and one that falls back to the
// generic pad. Open one directly with `?system=`.
const consoles = systemRegistry.systems.filter(
  (system, index, all) =>
    registry.profiles.some((profile) => profile.systems.includes(system.id)) ||
    (system.controllerProfile === "retropad" &&
      all.findIndex((other) => other.controllerProfile === "retropad") ===
        index),
);

// A developer-only entry page; Vite's production build uses index.html.
function Preview() {
  const [system, setSystem] = useState(
    new URLSearchParams(window.location.search).get("system") || "megadrive",
  );
  const [controls, setControls] = useState<Controls>(emptyControls);
  return (
    <main style={{ maxWidth: 1000, margin: "24px auto", padding: "0 24px" }}>
      <label>
        Console{" "}
        <select
          value={system}
          onChange={(e) => {
            setSystem(e.target.value);
            setControls(emptyControls());
          }}
        >
          {consoles.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name}
            </option>
          ))}
        </select>
      </label>
      <ControlsEditor
        system={system}
        value={controls}
        onChange={setControls}
        hotkeys={defaultHotkeys}
      />
    </main>
  );
}
createRoot(document.getElementById("root")!).render(<Preview />);
