import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import designs from "../designs.json";
import declared from "../defaults.json";

// The builder has the ROM-in-a-Box colours, from the default palette,
// whichever palette the author picks for a game.
const brand = designs.palettes.find((entry) => entry.id === declared.palette);
for (const [name, value] of Object.entries({ ...brand, ...brand?.tokens })) {
  if (typeof value === "string" && value.startsWith("#"))
    document.documentElement.style.setProperty(`--brand-${name}`, value);
}
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
