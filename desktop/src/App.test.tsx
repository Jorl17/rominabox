import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";

import { App } from "./App";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeAll(() => {
  if (
    !(Blob.prototype as Blob & { arrayBuffer?: () => Promise<ArrayBuffer> })
      .arrayBuffer
  ) {
    Object.defineProperty(Blob.prototype, "arrayBuffer", {
      configurable: true,
      value(this: Blob): Promise<ArrayBuffer> {
        return new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.onerror = () => reject(reader.error);
          reader.onload = () => resolve(reader.result as ArrayBuffer);
          reader.readAsArrayBuffer(this);
        });
      },
    });
  }
});

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  act(() => root.render(<App />));
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

function button(
  label: string,
  scope: ParentNode = container,
): HTMLButtonElement {
  const match = [...scope.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
  if (!(match instanceof HTMLButtonElement))
    throw new Error(`Missing button: ${label}`);
  return match;
}

function progressButton(label: string, scope: ParentNode): HTMLButtonElement {
  const match = scope.querySelector(`button[aria-label="${label}"]`);
  if (!(match instanceof HTMLButtonElement))
    throw new Error(`Missing progress button: ${label}`);
  return match;
}

function click(element: HTMLElement): void {
  element.dispatchEvent(new MouseEvent("click", { bubbles: true }));
}

function enterText(input: HTMLInputElement, value: string): void {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )?.set;
  setter?.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

async function openDetails(): Promise<void> {
  const bytes = new Uint8Array(512);
  [..."SEGA"].forEach((character, index) => {
    bytes[0x100 + index] = character.charCodeAt(0);
  });
  [..."TEST QUEST"].forEach((character, index) => {
    bytes[0x150 + index] = character.charCodeAt(0);
  });
  const file = new File([bytes.buffer], "upload.gen");
  const input = container.querySelector(
    'input[type="file"]',
  ) as HTMLInputElement;
  await act(async () => {
    Object.defineProperty(input, "files", {
      configurable: true,
      value: [file],
    });
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(container.textContent).toContain("upload.gen");
  await act(async () => {
    click(button("Next"));
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  expect(container.querySelector("h1")?.textContent).toBe("Game details");
}

async function openMenu(): Promise<void> {
  await openDetails();
  act(() => click(button("Next")));
  expect(container.querySelector("h1")?.textContent).toBe("Choose a menu");
}

describe("App workflow", () => {
  it("navigates through available progress steps without bypassing inspection", async () => {
    const progress = container.querySelector('nav[aria-label="Progress"]')!;
    expect(progressButton("Game", progress).getAttribute("aria-current")).toBe(
      "step",
    );
    expect(progressButton("Details", progress).disabled).toBe(true);
    expect(progressButton("Menu", progress).disabled).toBe(true);
    expect(progressButton("Export", progress).disabled).toBe(true);

    const bytes = new Uint8Array(512);
    [..."SEGA"].forEach((character, index) => {
      bytes[0x100 + index] = character.charCodeAt(0);
    });
    [..."TEST QUEST"].forEach((character, index) => {
      bytes[0x150 + index] = character.charCodeAt(0);
    });
    const input = container.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [new File([bytes.buffer], "upload.gen")],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });

    expect(progressButton("Details", progress).disabled).toBe(false);
    expect(progressButton("Menu", progress).disabled).toBe(true);
    await act(async () => {
      click(progressButton("Details", progress));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(container.querySelector("h1")?.textContent).toBe("Game details");
    expect(
      progressButton("Details", progress).getAttribute("aria-current"),
    ).toBe("step");
    expect(progressButton("Menu", progress).disabled).toBe(false);
    expect(progressButton("Export", progress).disabled).toBe(false);

    const name = container.querySelector(
      '.fields input:not([type="file"])',
    ) as HTMLInputElement;
    act(() => enterText(name, "My Test Quest"));
    act(() => click(progressButton("Game", progress)));
    act(() => click(progressButton("Menu", progress)));
    expect(container.querySelector("h1")?.textContent).toBe("Choose a menu");
    act(() => click(progressButton("Details", progress)));
    expect(
      (
        container.querySelector(
          '.fields input:not([type="file"])',
        ) as HTMLInputElement
      ).value,
    ).toBe("My Test Quest");
  });

  it("omits firmware authoring for a console with no firmware requirement", async () => {
    await openDetails();
    expect(container.textContent).not.toContain("Add BIOS files");
  });

  it("puts help outside scrolling content and outside checkbox activation", async () => {
    await openMenu();
    const help = container.querySelector(
      ".heading-row .help-button",
    ) as HTMLButtonElement;
    expect(help.closest("label")).toBeNull();
    act(() => help.focus());
    const tooltip = document.getElementById(
      help.getAttribute("aria-describedby")!,
    );
    expect(tooltip).not.toBeNull();
    expect(container.contains(tooltip)).toBe(false);
  });
  it("starts with only the game chooser and keeps Next disabled", () => {
    expect(container.querySelector("h1")?.textContent).toBe("Choose a game");
    expect(container.querySelector('input[type="file"]')).not.toBeNull();
    expect(container.textContent).not.toContain("App name");
    expect(button("Next").disabled).toBe(true);
  });

  it("inspects a local ROM, allows edits, and preserves them after Back", async () => {
    await openDetails();
    expect(container.textContent).toContain("Details from your file");
    expect(container.textContent).toContain("upload.gen");

    const name = container.querySelector(
      '.fields input:not([type="file"])',
    ) as HTMLInputElement;
    expect(name.value).toBe("TEST QUEST");
    act(() => enterText(name, "My Test Quest"));
    act(() => click(button("Next")));
    act(() => click(button("Back")));

    expect(
      (
        container.querySelector(
          '.fields input:not([type="file"])',
        ) as HTMLInputElement
      ).value,
    ).toBe("My Test Quest");
  });

  it("keeps menu options and startup logo state in the workflow", async () => {
    await openMenu();
    expect(container.querySelector("h1")?.textContent).toBe("Choose a menu");
    expect(
      container.querySelector('img[alt*="six save slots"]') as HTMLImageElement,
    ).not.toBeNull();
    expect(
      container.querySelector('[aria-label="Menu design"]'),
    ).not.toBeNull();

    const splash = [...container.querySelectorAll("label")]
      .find((label) => label.textContent?.includes("Startup logo"))
      ?.querySelector("input") as HTMLInputElement;
    expect(splash.checked).toBe(true);
    act(() => click(splash));
    expect(splash.checked).toBe(false);

    act(() => click(button("Next")));
    act(() => click(button("Back")));
    const restored = [...container.querySelectorAll("label")]
      .find((label) => label.textContent?.includes("Startup logo"))
      ?.querySelector("input") as HTMLInputElement;
    expect(restored.checked).toBe(false);
  });

  it("keeps advanced emulator access off unless the author opts in", async () => {
    await openMenu();
    const access = [...container.querySelectorAll("label")]
      .find((label) => label.textContent?.includes("Advanced emulator access"))
      ?.querySelector("input") as HTMLInputElement;
    expect(access.checked).toBe(false);
    act(() => click(access));
    expect(access.checked).toBe(true);

    act(() => click(button("Next")));
    act(() => click(button("Back")));
    const restored = [...container.querySelectorAll("label")]
      .find((label) => label.textContent?.includes("Advanced emulator access"))
      ?.querySelector("input") as HTMLInputElement;
    expect(restored.checked).toBe(true);
  });

  it("keeps shader choices inside advanced options and off by default", async () => {
    await openMenu();
    const advanced = [...container.querySelectorAll("details.advanced")].find(
      (details) =>
        details.querySelector("summary")?.textContent?.includes("Advanced"),
    );
    const scanlines = [...advanced!.querySelectorAll("label")]
      .find((label) => label.textContent?.includes("Scanlines"))
      ?.querySelector("input") as HTMLInputElement;
    expect(scanlines.checked).toBe(false);
    expect(advanced!.querySelector('[aria-label="Starts on"]')).toBeNull();
  });

  it("presents export as an honest disabled integration step", async () => {
    await openMenu();
    act(() => click(button("Next")));

    expect(container.querySelector("h1")?.textContent).toBe("Export your game");
    expect(container.textContent).toContain(
      "Export is available in the desktop app.",
    );
    expect(button("Create app").disabled).toBe(true);
  });
});
