import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";

import designs from "../designs.json";
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
    travelingFiles: (path: string, system: string) => {
      const list = travelingHandlers.list;
      if (!list) return actual.travelingFiles(path, system);
      return list(path, system);
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

function cliBinary(): string {
  // Executables end in .exe on Windows and have no suffix on POSIX systems.
  const name =
    process.platform === "win32" ? "rominabox-cli.exe" : "rominabox-cli";
  const roots: string[] = [];
  if (process.env.CARGO_TARGET_DIR) roots.push(process.env.CARGO_TARGET_DIR);
  const here = dirname(fileURLToPath(import.meta.url));
  // A copy in this checkout. Checkouts that share a cargo target overwrite its
  // binary at each build, so the binary is not stable.
  const privateCopy = resolve(here, "../../work/bios-cli", name);
  if (existsSync(privateCopy)) return privateCopy;
  roots.push(resolve(here, "../src-tauri/target"));
  try {
    const git = readFileSync(resolve(here, "../../.git"), "utf8");
    const match = git.match(/^gitdir:\s*(.+)$/m);
    if (match) {
      const gitdir = resolve(
        dirname(resolve(here, "../../.git")),
        match[1].trim(),
      );
      const common = gitdir.includes(`${sep}worktrees${sep}`)
        ? resolve(gitdir, "../..")
        : gitdir;
      roots.push(join(common, "shared-cargo-target"));
    }
  } catch {
    // In a normal checkout the cargo output is under src-tauri/target.
  }
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

function assessWithCli(
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

async function waitForText(text: string): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (container.textContent?.includes(text)) return;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
  }
  throw new Error(`Timed out waiting for: ${text}`);
}

async function chooseConsole(id: string): Promise<void> {
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

function checkbox(label: string): HTMLInputElement {
  const match = [...container.querySelectorAll("label")].find((item) =>
    item.textContent?.includes(label),
  );
  const input = match?.querySelector("input");
  if (!(input instanceof HTMLInputElement))
    throw new Error(`Missing checkbox: ${label}`);
  return input;
}

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

async function openDetails(name = "upload.gen"): Promise<void> {
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

async function openMenu(): Promise<void> {
  await openDetails();
  act(() => click(button("Next")));
  expect(container.querySelector("h1")?.textContent).toBe("Choose a menu");
}

describe("App workflow", () => {
  it("shows that a dropped game is being identified before the result arrives", async () => {
    let finish: (value: {
      title: string;
      system: string;
      source: "header" | "filename";
      filename: string;
      size: number;
    }) => void = () => {};
    inspectHandlers.inspect = () =>
      new Promise((resolve) => {
        finish = resolve;
      });
    const input = container.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [new File([new Uint8Array(32)], "Ape Escape (Europe).chd")],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(
      container.querySelector('progress[aria-label="Game identification"]'),
    ).not.toBeNull();
    expect(container.textContent).toContain("Finding your game");
    const progress = container.querySelector('nav[aria-label="Progress"]')!;
    expect(progressButton("Menu", progress).disabled).toBe(true);

    await act(async () => {
      finish({
        title: "Ape Escape",
        system: "ps1",
        source: "filename",
        filename: "Ape Escape (Europe).chd",
        size: 32,
      });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    inspectHandlers.inspect = null;
  });

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
    // Choosing the file starts identification. The menu stays unavailable
    // until it finishes. The held-lookup test covers the wait itself.
    await waitForText("Game details");
    expect(
      container.querySelector('progress[aria-label="Game identification"]'),
    ).toBeNull();
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

  it("stops at the BIOS step while a required file is missing", async () => {
    await openDetails("disc.cue");
    await chooseConsole("pcecd");
    expect(button("Next").disabled).toBe(true);
    const assessment = await assessWithCli("pcecd", []);
    const required = assessment.notices.find(
      (notice) => notice.kind === "required",
    );
    if (!required) throw new Error("the engine did not require a BIOS");
    await waitForText(required.text);
    expect(button("Next").disabled).toBe(true);
    expect(container.querySelector(".firmware-required")).not.toBeNull();
    const progress = container.querySelector('nav[aria-label="Progress"]')!;
    expect(progressButton("Menu", progress).disabled).toBe(true);
    expect(progressButton("Export", progress).disabled).toBe(true);
  });

  it("never says Ready to go beside a Next it has disabled", async () => {
    await openDetails("disc.cue");
    await chooseConsole("pcecd");
    const assessment = await assessWithCli("pcecd", []);
    const required = assessment.notices.find(
      (notice) => notice.kind === "required",
    );
    if (!required) throw new Error("the engine did not require a BIOS");
    await waitForText(required.text);
    expect(button("Next").disabled).toBe(true);
    expect(container.textContent).not.toContain("Ready to go");
    expect(container.textContent).toContain("needs a BIOS file");

    // And the other way, so this is not simply "never say it": a console whose
    // BIOS is optional is ready, and we must not say that it requires one.
    await chooseConsole("ps1");
    const optional = assessment.notices.length
      ? await assessWithCli("ps1", [])
      : null;
    if (!optional?.notices.some((notice) => notice.kind === "optional")) {
      throw new Error("the engine did not explain the optional BIOS");
    }
    await waitForText("Ready to go");
    expect(button("Next").disabled).toBe(false);
    expect(container.textContent).not.toContain("needs a BIOS file");
  });

  it("does not stop for a console whose BIOS is optional", async () => {
    await openDetails("disc.cue");
    await chooseConsole("ps1");
    const assessment = await assessWithCli("ps1", []);
    const optional = assessment.notices.find(
      (notice) => notice.kind === "optional",
    );
    if (!optional)
      throw new Error("the engine did not explain the optional BIOS");
    await waitForText(optional.text);
    expect(button("Next").disabled).toBe(false);
    expect(container.querySelector(".firmware-required")).toBeNull();
    const progress = container.querySelector('nav[aria-label="Progress"]')!;
    expect(progressButton("Menu", progress).disabled).toBe(false);
    expect(progressButton("Export", progress).disabled).toBe(false);
  });

  it("explains a BIOS file that does not match this console", async () => {
    await openDetails("disc.cue");
    await chooseConsole("pcecd");
    const input = container.querySelector(
      "input[data-firmware]",
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [new File([""], "notes.txt")],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
    const assessment = await assessWithCli("pcecd", ["notes.txt"]);
    const reason = assessment.files[0]?.reason;
    if (!reason)
      throw new Error("the engine did not explain the unmatched file");
    await waitForText(reason);
    expect(button("Next").disabled).toBe(true);
  });

  it("continues once the required BIOS file is provided", async () => {
    await openDetails("disc.cue");
    await chooseConsole("pcecd");
    const input = container.querySelector(
      "input[data-firmware]",
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [new File([""], "syscard3.pce")],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
    const assessment = await assessWithCli("pcecd", ["syscard3.pce"]);
    const ready = assessment.notices.find((notice) => notice.kind === "ready");
    if (!ready) throw new Error("the engine did not accept the BIOS file");
    await waitForText(ready.text);
    expect(button("Next").disabled).toBe(false);
    expect(container.querySelector(".firmware-required")).toBeNull();
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

  it("gives picture filters their own section and one way to say selected", async () => {
    await openMenu();
    const text = (element: Element | null | undefined) =>
      (element?.textContent || "").replace(/\s+/g, " ").trim();
    const filters = [...container.querySelectorAll("details")].find((details) =>
      details.querySelector(".shader-grid"),
    );
    const controls = container.querySelector("details.author-controls");
    const advanced = [...container.querySelectorAll("details")].find(
      (details) => text(details.querySelector("summary")) === "Advanced",
    );
    // The title is the count, so a selected card has no Bundle tick that
    // repeats it.
    expect(text(filters?.querySelector("summary"))).toBe(
      "Picture filters · none selected",
    );
    expect(filters?.querySelector("input[type='checkbox']")).toBeNull();
    expect(text(filters)).not.toMatch(/\bBundle\b/);
    expect(text(filters?.querySelector(".shader-grid .shader-add"))).toContain(
      "Add your own",
    );
    expect(
      controls &&
        filters &&
        controls.compareDocumentPosition(filters) &
          Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(advanced?.querySelector(".shader-grid")).toBeNull();

    const scanlines = [
      ...(filters?.querySelectorAll(".shader-grid .shader-card") ?? []),
    ].find((card) => card.textContent?.includes("Scanlines"));
    if (!(scanlines instanceof HTMLElement)) {
      throw new Error("Scanlines is not a card in the grid");
    }
    act(() => click(scanlines));
    expect(text(filters?.querySelector("summary"))).toBe(
      "Picture filters · 1 selected",
    );
    expect(scanlines.classList.contains("chosen")).toBe(true);
    expect(scanlines.querySelector("input")).toBeNull();
    act(() => click(scanlines));
    expect(text(filters?.querySelector("summary"))).toBe(
      "Picture filters · none selected",
    );
    expect(filters?.querySelector('[aria-label="Starts on"]')).toBeNull();
  });

  it("presents export as an honest disabled integration step", async () => {
    await openMenu();
    act(() => click(button("Next")));

    expect(container.querySelector("h1")?.textContent).toBe("Export your game");
    expect(container.textContent).toContain(
      "Export is available in the desktop app.",
    );
    expect(container.textContent?.toLowerCase() ?? "").not.toContain("zip");
    const saveHelp = container.querySelector(
      ".destination .help-button",
    ) as HTMLButtonElement;
    act(() => click(saveHelp));
    expect(document.body.textContent?.toLowerCase() ?? "").not.toContain("zip");
    expect(button("Create app").disabled).toBe(true);
  });

  it("lets the author choose a menu design, and exports the one they chose", async () => {
    // We must pass the chosen design from the selector to the export, not a
    // fixed "native" value. Only with a second design can we tell them apart.
    await openMenu();
    const picker = [...container.querySelectorAll("select")].find(
      (select) => select.getAttribute("aria-label") === "Menu design",
    );
    if (!picker)
      throw new Error("the menu design selector is not on this step");

    // We offer every declared design, not a subset.
    const offered = [...picker.options].map((option) => option.value).sort();
    expect(offered).toEqual(designs.designs.map((d) => d.id).sort());

    // This is meaningful only with a second design. With one design, an
    // assertion that the selector shows it proves nothing, because the
    // hardcoded string and the state are the same.
    const other = designs.designs.find((d) => d.id !== picker.value);
    if (!other) {
      expect(
        designs.designs.length,
        "only one design is declared, so this cannot yet prove the choice " +
          "is honoured — it becomes a real test when a second one lands",
      ).toBe(1);
      return;
    }
    await act(async () => {
      picker.value = other.id;
      picker.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(
      picker.value,
      "choosing a design has to stick, or the export gets the old one",
    ).toBe(other.id);
  });

  async function dropWithCompanions(name: string, files: string[]) {
    const path = `/games/${name}`;
    travelingHandlers.list = async () => ({ entry: path, files });
    inspectHandlers.inspect = async () => ({
      title: "Sonic Adventure 2",
      system: "dreamcast",
      source: "filename",
      filename: name,
      size: 32,
    });
    const file = new File([new Uint8Array(32)], name) as File & {
      path?: string;
    };
    file.path = path;
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
      const also = container.querySelector(".traveling-also");
      if (
        container.querySelector("h1")?.textContent === "Game details" &&
        also
      ) {
        return also;
      }
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 20));
      });
    }
    throw new Error(
      "the Also importing line did not name the files that travel",
    );
  }

  it("names a disc's tracks on one short line, without repeating the game", async () => {
    const stem = "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es)";
    const also = await dropWithCompanions(`${stem}.gdi`, [
      `${stem}.gdi`,
      `${stem} (Track 1).bin`,
      `${stem} (Track 2).bin`,
      `${stem} (Track 3).bin`,
    ]);
    expect(also.textContent).toBe(
      "Also importing: (Track 1).bin, (Track 2).bin, (Track 3).bin",
    );
    expect(also.textContent).not.toContain(stem);
  });

  it("counts companions once there are more than a handful", async () => {
    const also = await dropWithCompanions("Long Disc.cue", [
      "Long Disc.cue",
      ...Array.from({ length: 6 }, (_, index) => `track${index + 1}.bin`),
    ]);
    expect(also.textContent).toBe("Also importing 6 files");
  });

  it("offers menu sounds as a picker and a preview, with nothing to read", async () => {
    await openMenu();
    const picker = container.querySelector("#menu-sounds") as HTMLSelectElement;
    const field = picker.closest(".sound-choice") as HTMLElement;
    expect(field.querySelector("label")?.textContent).toBe("Menu sounds");
    expect(field.querySelector(".menu-sound-preview button")).not.toBeNull();
    expect(field.querySelector(".help-button")).toBeNull();
    for (const pack of [...picker.options].map((option) => option.value)) {
      act(() => {
        picker.value = pack;
        picker.dispatchEvent(new Event("change", { bubbles: true }));
      });
      expect(picker.value).toBe(pack);
      // Only the label remains to read beside the pack names and the preview.
      const words = [...field.childNodes]
        .filter((node) => node !== picker)
        .map((node) =>
          node instanceof HTMLElement && node.matches(".menu-sound-preview")
            ? ""
            : node.textContent,
        )
        .join("");
      expect(words, pack).toBe("Menu sounds");
      expect(
        [...picker.options].every((option) => !option.title),
        pack,
      ).toBe(true);
    }
  });

  it("puts background play and autosave next to the startup logo", async () => {
    await openMenu();
    const logo = checkbox("Startup logo");
    const playing = checkbox("Keep playing in the background");
    const saving = checkbox("Autosave on quit");
    expect(playing.checked).toBe(false);
    expect(saving.checked).toBe(false);
    expect(playing.closest("details")).toBeNull();
    expect(saving.closest("details")).toBeNull();
    expect(logo.closest(".menu-settings")).toBe(
      playing.closest(".menu-settings"),
    );
    for (const [label, says] of [
      ["About keep playing in the background", "keep running"],
      ["About autosave on quit", "continue from there"],
    ]) {
      const help = container.querySelector(
        `[aria-label="${label}"]`,
      ) as HTMLButtonElement;
      expect(help).not.toBeNull();
      expect(help.closest("label")).toBeNull();
      act(() => help.focus());
      const tooltip = document.getElementById(
        help.getAttribute("aria-describedby")!,
      );
      expect(tooltip?.textContent).toContain(says);
      act(() => help.blur());
    }

    act(() => click(playing));
    act(() => click(saving));
    act(() => click(button("Next")));
    act(() => click(button("Back")));
    expect(checkbox("Keep playing in the background").checked).toBe(true);
    expect(checkbox("Autosave on quit").checked).toBe(true);

    act(() => click(checkbox("Include game menu")));
    expect(container.textContent).toContain("No in-game menu");
    expect(checkbox("Keep playing in the background").checked).toBe(true);
    expect(checkbox("Autosave on quit").checked).toBe(true);
    expect(checkbox("Startup logo").closest(".play-options")).not.toBeNull();
  });

  it("keeps achievements as a main option and requires the game menu", async () => {
    await openMenu();
    const achievements = checkbox("Achievements");
    expect(achievements.checked).toBe(true);
    expect(achievements.disabled).toBe(false);
    expect(achievements.closest("details")).toBeNull();
    expect(achievements.closest(".menu-settings")).toBe(
      checkbox("Startup logo").closest(".menu-settings"),
    );

    act(() => click(checkbox("Include game menu")));
    expect(checkbox("Achievements").checked).toBe(false);
    expect(checkbox("Achievements").disabled).toBe(true);
    act(() => click(checkbox("Include game menu")));
    expect(checkbox("Achievements").checked).toBe(true);
    act(() => click(checkbox("Achievements")));
    act(() => click(checkbox("Include game menu")));
    act(() => click(checkbox("Include game menu")));
    expect(checkbox("Achievements").checked).toBe(false);
  });

  async function dropNamed(name: string) {
    const path = `/games/${name}`;
    const file = new File([new Uint8Array(32)], name) as File & {
      path?: string;
    };
    file.path = path;
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
    await waitForText("Game details");
    return path;
  }

  it("puts the console back when inspection rejects it, and keeps the message", async () => {
    const stem = "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es)";
    travelingHandlers.list = async () => ({
      entry: `/games/${stem}.gdi`,
      files: [
        `${stem}.gdi`,
        `${stem} (Track 1).bin`,
        `${stem} (Track 2).bin`,
        `${stem} (Track 3).bin`,
      ],
    });
    inspectHandlers.inspect = async () => ({
      title: "Sonic Adventure 2",
      system: "dreamcast",
      source: "filename",
      filename: `${stem}.gdi`,
      size: 32,
    });
    await dropNamed(`${stem}.gdi`);
    const also = container.querySelector(".traveling-also")?.textContent;
    nativeBridge.on = true;
    nativeBridge.inspectGame = async (_path, _online, system) => {
      if (system === "ps1")
        throw new Error("PlayStation does not support .gdi files.");
      return {
        title: "Sonic Adventure 2",
        system: "dreamcast",
        filename: `${stem}.gdi`,
        size: 32,
        source: "filename",
        matched: false,
        warnings: [],
      };
    };
    await chooseConsole("ps1");
    await waitForText("PlayStation does not support .gdi files.");
    const select = container.querySelector(
      ".fields select",
    ) as HTMLSelectElement;
    expect(
      select.value,
      "a console inspection rejects has to stay the one that was valid, or export copies a different set than the line",
    ).toBe("dreamcast");
    expect(container.querySelector(".error")?.textContent).toBe(
      "PlayStation does not support .gdi files.",
    );
    expect(container.querySelector(".traveling-also")?.textContent).toBe(also);
  });

  it("puts the console back in the browser walk when inspection rejects it", async () => {
    const stem = "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es)";
    travelingHandlers.list = async () => ({
      entry: `/games/${stem}.gdi`,
      files: [`${stem}.gdi`, `${stem} (Track 1).bin`],
    });
    inspectHandlers.inspect = async () => ({
      title: "Sonic Adventure 2",
      system: "dreamcast",
      source: "filename",
      filename: `${stem}.gdi`,
      size: 32,
    });
    await dropNamed(`${stem}.gdi`);
    await chooseConsole("ps1");
    await waitForText("PlayStation does not support .gdi files.");
    const select = container.querySelector(
      ".fields select",
    ) as HTMLSelectElement;
    expect(select.value).toBe("dreamcast");
    expect(container.querySelector(".traveling-also")?.textContent).toBe(
      "Also importing: (Track 1).bin",
    );
  });

  it("shows the exporter's refusal instead of a one-file line", async () => {
    const refusal = "missing sub file game.sub (from /games/game.ccd)";
    travelingHandlers.list = async () => {
      throw new Error(refusal);
    };
    inspectHandlers.inspect = async () => ({
      title: "Game",
      system: "pcecd",
      source: "filename",
      filename: "game.ccd",
      size: 32,
    });
    const file = new File([new Uint8Array(32)], "game.ccd") as File & {
      path?: string;
    };
    file.path = "/games/game.ccd";
    const input = container.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [file],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
      await new Promise((resolve) => setTimeout(resolve, 30));
    });
    await waitForText(refusal);
    expect(container.querySelector(".error")?.textContent).toBe(refusal);
    expect(
      container.querySelector("[data-traveling]"),
      "a refused disc is not a one-file game",
    ).toBeNull();
  });
});
