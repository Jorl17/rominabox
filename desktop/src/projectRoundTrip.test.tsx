import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ExportRequest } from "./bridge";

// The desktop shell, answered here: we open the project below in the builder
// and pass whatever we save to `saved`.
const shell = vi.hoisted(() => ({
  opened: null as Omit<ExportRequest, "outputDir"> | null,
  saved: [] as ExportRequest[],
}));
vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./bridge")>();
  return {
    ...actual,
    native: true,
    onNativeDrop: () => Promise.resolve(() => {}),
    onExportProgress: () => Promise.resolve(() => {}),
    onAboutRequested: () => Promise.resolve(() => {}),
    defaultDestination: () => Promise.resolve("/Games"),
    exportTarget: () => Promise.resolve("macos"),
    assessFirmware: () =>
      Promise.resolve({ canContinue: true, notices: [], files: [] }),
    travelingFiles: (path: string) =>
      Promise.resolve({ entry: path, files: [] }),
    readImage: (path: string) => Promise.resolve({ path, url: `blob:${path}` }),
    pickProjectOpen: () => Promise.resolve("/Projects/Every Setting.rominabox"),
    openProject: () =>
      Promise.resolve({ settings: shell.opened, extractionDir: "/opened" }),
    pickProjectSave: () => Promise.resolve("/Projects/Again.rominabox"),
    saveProject: (archivePath: string, settings: ExportRequest) => {
      shell.saved.push(settings);
      return Promise.resolve({ archivePath, archiveBytes: 1 });
    },
  };
});
import { App } from "./App";
(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

/** Every setting a project keeps, none of them the builder's default. */
const everySetting = {
  rom: "/opened/content/Game.md",
  files: { leftOut: [], added: ["/opened/content/Manual.txt"] },
  title: "Every Setting",
  system: "megadrive",
  icon: "/opened/assets/icon.png",
  background: "/opened/assets/background.png",
  tintBackground: true,
  showMenu: true,
  startAtMenu: true,
  theme: "disc",
  palette: "amber",
  menuSounds: "bell",
  controls: { bindings: { a: { label: "Jump", key: "space" } } },
  hotkeys: {
    menu: ["key:f1", "pad:home"],
    confirm: ["key:enter"],
    back: ["key:backspace", "pad:b"],
    "previous-page": ["key:pageup"],
    "next-page": ["pad:r"],
    "quick-save": ["key:f5", "pad:r2"],
    "quick-load": ["key:f8"],
    "previous-slot": [],
    "next-slot": ["key:f12"],
    "fast-forward": ["key:tab"],
    fullscreen: ["key:f11"],
  },
  firmware: ["/opened/firmware/bios.bin"],
  splash: false,
  includeAchievements: false,
  advancedEmulatorAccess: true,
  keepPlayingInBackground: true,
  fastForward: true,
  fastForwardSpeed: 4,
  video: true,
  gameData: false,
  brightness: 1.2,
  contrast: 0.9,
  fastForwardHold: false,
  autosaveOnQuit: true,
  everyPadIsPlayerOne: false,
  menuEntries: ["controls"],
  shaders: {
    bundled: ["scanlines", "phosphor"],
    custom: [{ name: "Mine", path: "/opened/shaders/mine.glsl" }],
    initial: "phosphor",
  },
  target: "windows",
  intelMacs: true,
} as Omit<ExportRequest, "outputDir">;

let cleanup = () => {};
afterEach(() => {
  cleanup();
  shell.saved.length = 0;
});

async function settle() {
  await act(async () => {
    for (let turn = 0; turn < 20; turn += 1) await Promise.resolve();
  });
}

function pressButton(container: HTMLElement, words: string) {
  const button = [...container.querySelectorAll("button")].find(
    (each) =>
      each.textContent?.trim() === words ||
      each.getAttribute("aria-label") === words,
  );
  if (!button) throw new Error(`No ${words} button`);
  act(() => button.click());
}

/** What we save in the builder after opening `opened` and going to Export. */
async function reopenAndSave(opened: Omit<ExportRequest, "outputDir">) {
  shell.opened = opened;
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  cleanup = () => {
    act(() => root.unmount());
    container.remove();
  };
  act(() => root.render(<App />));
  await settle();
  pressButton(container, "Open project");
  await settle();
  pressButton(container, "Export");
  await settle();
  pressButton(container, "Save project");
  await settle();
  expect(shell.saved).toHaveLength(1);
  const { outputDir, ...saved } = shell.saved[0];
  void outputDir;
  return saved;
}

describe("a project in the builder", () => {
  it("saves again with every setting it was opened with", async () => {
    expect(await reopenAndSave(everySetting)).toEqual({
      ...everySetting,
      bothPlatforms: false,
    });
  });

  it("stays a game for Mac and Windows", async () => {
    const both = { ...everySetting, target: "macos", bothPlatforms: true };
    expect(await reopenAndSave(both as typeof everySetting)).toEqual(both);
  });
});
