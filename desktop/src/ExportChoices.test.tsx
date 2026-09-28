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
  host: ExportTarget | null = target,
  onTarget = (_: ExportTarget) => {},
) {
  act(() =>
    root.render(
      <ExportChoices
        host={host}
        target={target}
        onTarget={onTarget}
        destination="/Volumes/Games/ROM-in-a-Box"
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

  it("makes the game for this machine unless the author opens Change", () => {
    const chosen = vi.fn();
    const intel = vi.fn();
    show("macos", false, intel, "macos", chosen);
    const row = container.querySelector(".platform");
    expect(row?.textContent).toContain("Made for");
    expect(row?.textContent).toContain("This Mac");
    expect(container.querySelectorAll("input[type='radio']")).toHaveLength(0);

    const change = row?.querySelector(":scope > button") as HTMLButtonElement;
    act(() => change?.click());
    const options = [...container.querySelectorAll(".platform-choices label")];
    expect(options.map((option) => option.textContent)).toEqual([
      "This Mac",
      "Windows",
    ]);
    act(() => options[1].querySelector("input")?.click());
    expect(chosen).toHaveBeenCalledWith("windows");
    expect(intel).not.toHaveBeenCalled();
    expect(container.querySelectorAll("input[type='radio']")).toHaveLength(0);
  });

  it("offers a PC its own platform first, and a Mac game made there for every Mac", () => {
    const chosen = vi.fn();
    const intel = vi.fn();
    show("windows", false, intel, "windows", chosen);
    expect(container.querySelector(".platform")?.textContent).toContain(
      "This PC",
    );
    act(() =>
      container.querySelector<HTMLButtonElement>(".platform > button")?.click(),
    );
    const options = [...container.querySelectorAll(".platform-choices label")];
    expect(options.map((option) => option.textContent)).toEqual([
      "This PC",
      "Mac",
    ]);
    act(() => options[1].querySelector("input")?.click());
    expect(chosen).toHaveBeenCalledWith("macos");
    expect(intel).toHaveBeenCalledWith(true);
  });

  it("shows no platform before it knows this machine's", () => {
    show(null, false, () => {}, null);
    expect(container.querySelector(".platform")).toBeNull();
  });
});
