import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type * as bridge from "./bridge";
import { AboutDialog } from "./AboutDialog";

// The licence index and texts of this repository, bundled with the builder.
const LICENCES = join(__dirname, "..", "..", "licenses");
const opened: string[] = [];
vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof bridge>();
  return {
    ...actual,
    aboutComponents: async () =>
      JSON.parse(readFileSync(join(LICENCES, "index.json"), "utf-8")),
    licenceText: async (file: string) => {
      opened.push(file);
      return readFileSync(join(LICENCES, file), "utf-8");
    },
    appVersion: async () => "0.1.0",
  };
});

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  opened.length = 0;
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

async function shown(onClose = () => {}) {
  await act(async () => root.render(<AboutDialog onClose={onClose} />));
}

describe("the builder's About", () => {
  it("lists every component of the index, grouped, with its licence", async () => {
    await shown();
    const rows = JSON.parse(
      readFileSync(join(LICENCES, "index.json"), "utf-8"),
    ) as bridge.Component[];
    expect(container.querySelector("h2")?.textContent).toBe(
      "ROM-in-a-Box 0.1.0",
    );
    expect(container.querySelectorAll(".about-entry")).toHaveLength(
      rows.length,
    );
    const headings = [...container.querySelectorAll("h3")].map(
      (heading) => heading.textContent,
    );
    expect(headings).toEqual([
      "Game player",
      "Emulator cores",
      "Data",
      "Fonts",
      "Rust crates",
      "Interface packages",
      "Toolchains",
    ]);
    const retroarch = [...container.querySelectorAll("summary")].find(
      (summary) =>
        summary.querySelector(".about-title")?.textContent === "RetroArch",
    );
    expect(retroarch?.querySelector(".about-licence")?.textContent).toBe(
      "GPL-3.0-or-later",
    );
    expect(opened).toEqual([]);
  });

  it("reads a licence text only when its component is opened", async () => {
    await shown();
    const entry = [
      ...container.querySelectorAll<HTMLDetailsElement>(".about-entry"),
    ].find(
      (details) =>
        details.querySelector(".about-title")?.textContent === "RetroArch",
    )!;
    await act(async () => {
      entry.open = true;
      entry.dispatchEvent(new Event("toggle"));
    });
    expect(opened).toEqual(["native/retroarch.txt"]);
    expect(entry.querySelector(".about-text")?.textContent).toContain(
      "GNU GENERAL PUBLIC LICENSE",
    );
  });

  it("closes with Close and with Escape", async () => {
    const onClose = vi.fn();
    await shown(onClose);
    await act(async () =>
      container
        .querySelector<HTMLButtonElement>(".pop-up-actions button")!
        .click(),
    );
    await act(async () =>
      container
        .querySelector(".pop-up")!
        .dispatchEvent(
          new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
        ),
    );
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
