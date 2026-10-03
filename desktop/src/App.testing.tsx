// What every App.*.test.tsx uses: the stand-ins for the bridge and for
// inspection, a fresh App rendered before each test, and the steps through
// it. Import this in a test file before anything that imports ./App or
// ./bridge, so the stand-ins are in place first.
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, vi } from "vitest";

import { App } from "./App";
import { type FirmwareAssessment } from "./bridge";

const { firmwareHandlers, inspectHandlers, travelingHandlers, nativeBridge } =
  vi.hoisted(() => ({
    firmwareHandlers: {
      assess: null as
        | ((system: string, files: string[]) => Promise<FirmwareAssessment>)
        | null,
    },
    inspectHandlers: {
      inspect: null as
        | ((file: File) => Promise<{
            title: string;
            system: string;
            source: "header" | "filename";
            filename: string;
            size: number;
          }>)
        | null,
    },
    travelingHandlers: {
      list: null as
        | ((
            path: string,
            system: string,
          ) => Promise<{
            entry: string;
            files: string[];
          }>)
        | null,
    },
    nativeBridge: {
      on: false,
      inspectGame: null as
        | ((
            path: string,
            online: boolean,
            system?: string,
          ) => Promise<import("./bridge").GameInfo>)
        | null,
    },
  }));
export { inspectHandlers, nativeBridge, travelingHandlers };

vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./bridge")>();
  return {
    ...actual,
    get native() {
      return nativeBridge.on;
    },
    inspectGame: (path: string, online: boolean, systemOverride?: string) => {
      if (nativeBridge.inspectGame)
        return nativeBridge.inspectGame(path, online, systemOverride);
      return actual.inspectGame(path, online, systemOverride);
    },
    assessFirmware: (system: string, files: string[]) => {
      const assess = firmwareHandlers.assess;
      if (!assess) return actual.assessFirmware(system, files);
      return assess(system, files);
    },
    // A test lists the files, and in the answer from the exporter each has a
    // role, with the game file first.
    travelingFiles: async (path: string, system: string) => {
      const list = travelingHandlers.list;
      if (!list) return actual.travelingFiles(path, system);
      const listed = await list(path, system);
      return {
        entry: listed.entry,
        files: listed.files.map((name, index) => ({
          name,
          role: { kind: index === 0 ? "game" : "named" } as const,
        })),
        patches: [],
        refused: [],
        added: [],
      };
    },
  };
});

vi.mock("./inspection", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./inspection")>();
  return {
    ...actual,
    inspectRom: (
      file: File,
      onProgress?: (label: string) => void,
      systemOverride?: string,
    ) => {
      if (systemOverride)
        return actual.inspectRom(file, onProgress, systemOverride);
      const inspect = inspectHandlers.inspect;
      if (inspect) return inspect(file);
      return actual.inspectRom(file, onProgress, systemOverride);
    },
  };
});

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

export let container: HTMLDivElement;
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

function cliBinary(): string {
  // Executables end in .exe on Windows and have no suffix on POSIX systems.
  const name =
    process.platform === "win32" ? "rominabox-cli.exe" : "rominabox-cli";
  const roots: string[] = [];
  if (process.env.CARGO_TARGET_DIR) roots.push(process.env.CARGO_TARGET_DIR);
  const here = dirname(fileURLToPath(import.meta.url));
  roots.push(resolve(here, "../target"));
  for (const root of roots) {
    for (const profile of ["debug", "release"]) {
      const candidate = join(root, profile, name);
      if (existsSync(candidate)) return candidate;
    }
  }
  throw new Error(
    "rominabox-cli is not built, so the builder cannot ask whether a BIOS is required",
  );
}

export function assessWithCli(
  system: string,
  files: string[],
): Promise<FirmwareAssessment> {
  const stdout = execFileSync(cliBinary(), ["firmware"], {
    input: JSON.stringify({ system, files }),
    encoding: "utf8",
  });
  const line = stdout
    .trim()
    .split("\n")
    .find((row: string) => row.startsWith("{"));
  if (!line) throw new Error(stdout);
  const parsed = JSON.parse(line) as {
    type: string;
    result?: FirmwareAssessment;
    message?: string;
  };
  if (parsed.type !== "result" || !parsed.result) {
    throw new Error(parsed.message || stdout);
  }
  return Promise.resolve(parsed.result);
}

export async function waitForText(text: string): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (container.textContent?.includes(text)) return;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
  }
  throw new Error(`Timed out waiting for: ${text}`);
}

export async function chooseConsole(id: string): Promise<void> {
  const select = container.querySelector(".fields select") as HTMLSelectElement;
  const setter = Object.getOwnPropertyDescriptor(
    HTMLSelectElement.prototype,
    "value",
  )?.set;
  setter?.call(select, id);
  await act(async () => {
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  for (let attempt = 0; attempt < 40; attempt += 1) {
    const current = container.querySelector(
      ".fields select",
    ) as HTMLSelectElement | null;
    if (current?.value === id || container.querySelector(".error")) return;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 15));
    });
  }
}

beforeEach(() => {
  firmwareHandlers.assess = assessWithCli;
  inspectHandlers.inspect = null;
  travelingHandlers.list = null;
  nativeBridge.on = false;
  nativeBridge.inspectGame = null;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  act(() => root.render(<App />));
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

export function checkbox(label: string): HTMLInputElement {
  const match = [...container.querySelectorAll("label")].find((item) =>
    item.textContent?.includes(label),
  );
  const input = match?.control;
  if (!(input instanceof HTMLInputElement))
    throw new Error(`Missing checkbox: ${label}`);
  return input;
}

export function button(
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

export function progressButton(
  label: string,
  scope: ParentNode,
): HTMLButtonElement {
  const match = scope.querySelector(`button[aria-label="${label}"]`);
  if (!(match instanceof HTMLButtonElement))
    throw new Error(`Missing progress button: ${label}`);
  return match;
}

export function click(element: HTMLElement): void {
  element.dispatchEvent(new MouseEvent("click", { bubbles: true }));
}

export function enterText(input: HTMLInputElement, value: string): void {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )?.set;
  setter?.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

export async function openDetails(name = "upload.gen"): Promise<void> {
  const bytes = new Uint8Array(512);
  [..."SEGA"].forEach((character, index) => {
    bytes[0x100 + index] = character.charCodeAt(0);
  });
  [..."TEST QUEST"].forEach((character, index) => {
    bytes[0x150 + index] = character.charCodeAt(0);
  });
  const file = new File([bytes.buffer], name);
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
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const looking = container.querySelector(
      'progress[aria-label="Game identification"]',
    );
    if (
      container.querySelector("h1")?.textContent === "Game details" &&
      !looking
    ) {
      expect(container.textContent).toContain(name);
      return;
    }
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
  }
  throw new Error("choosing a game did not finish identifying it");
}

export async function openMenu(): Promise<void> {
  await openDetails();
  act(() => click(button("Next")));
  expect(container.querySelector("h1")?.textContent).toBe("Choose a menu");
}
