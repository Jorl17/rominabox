import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { ControlsEditor, emptyControls, type Controls } from "./controls";
import "./style.css";

// A developer-only entry page; Vite's production build uses index.html.
function Preview() {
  const [system, setSystem] = useState("megadrive");
  const [controls, setControls] = useState<Controls>(emptyControls);
  return (
    <main style={{ maxWidth: 1000, margin: "24px auto", padding: "0 24px" }}>
      <label>
        Controller{" "}
        <select
          value={system}
          onChange={(e) => {
            setSystem(e.target.value);
            setControls(emptyControls());
          }}
        >
          <option value="megadrive">Mega Drive</option>
          <option value="gbc">Game Boy</option>
          <option value="snes">Super Nintendo</option>
          <option value="ps1">Generic RetroPad</option>
        </select>
      </label>
      <ControlsEditor system={system} value={controls} onChange={setControls} />
    </main>
  );
}
createRoot(document.getElementById("root")!).render(<Preview />);
