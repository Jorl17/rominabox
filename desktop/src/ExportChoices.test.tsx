import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import declared from "../defaults.json";
import { type ExportTarget } from "./bridge";
import { ExportChoices } from "./ExportChoices";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

function show(
  target: ExportTarget | null,
  intelMacs: boolean,
  onIntelMacs = (_: boolean) => {},
) {
  act(() =>
    root.render(
      <ExportChoices
        target={target}
        destination="/Users/player/Downloads/ROM-in-a-Box"
        onDestination={() => {}}
        intelMacs={intelMacs}
        onIntelMacs={onIntelMacs}
        fail={() => {}}
      />,
    ),
  );
}

function intelChoice(): HTMLInputElement | null {
  const label = [...container.querySelectorAll("label")].find(
    (element) => element.textContent === "Also runs on Intel Macs",
  );
  return label?.querySelector("input[type='checkbox']") ?? null;
}

describe("the export step's choices", () => {
  it("offers Intel Macs for a Mac app, off unless the author turns it on", () => {
    expect(declared.intelMacs).toBe(false);
    const said = vi.fn();
    show("macos", declared.intelMacs, said);
    const choice = intelChoice();
    expect(choice?.checked).toBe(false);
    act(() => choice?.click());
    expect(said).toHaveBeenCalledWith(true);

    show("macos", true, said);
    expect(intelChoice()?.checked).toBe(true);
    act(() => intelChoice()?.click());
    expect(said).toHaveBeenLastCalledWith(false);
  });

  it("does not offer Intel Macs for a Windows app, or before the platform is known", () => {
    for (const target of ["windows", null] as const) {
      show(target, false);
      expect(intelChoice()).toBeNull();
      expect(container.textContent).not.toContain("Intel");
      expect(container.textContent).toContain("Save to");
    }
  });
});
