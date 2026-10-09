import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

// The desktop shell, answered here: we keep the window's drop callback and
// the games the builder starts to identify.
const shell = vi.hoisted(() => ({
  drop: (() => {}) as (
    paths: string[],
    position: { x: number; y: number },
  ) => void,
  identified: [] as string[],
}));
vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./bridge")>();
  return {
    ...actual,
    native: true,
    onNativeDrop: (callback: typeof shell.drop) => {
      shell.drop = callback;
      return Promise.resolve(() => {});
    },
    onExportProgress: () => Promise.resolve(() => {}),
    onAboutRequested: () => Promise.resolve(() => {}),
    defaultDestination: () => Promise.resolve("/Games"),
    exportTarget: () => Promise.resolve("macos"),
    games: () => Promise.resolve([]),
    inspectGame: (path: string) => {
      shell.identified.push(path);
      return new Promise(() => {});
    },
  };
});
import { App } from "./App";
(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let cleanup = () => {};
afterEach(() => cleanup());

async function settle() {
  await act(async () => {
    for (let turn = 0; turn < 20; turn += 1) await Promise.resolve();
  });
}

describe("a file dropped on the window", () => {
  it("is not a game for the builder while the Game data section covers it", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const elementFromPoint = document.elementFromPoint;
    cleanup = () => {
      document.elementFromPoint = elementFromPoint;
      act(() => root.unmount());
      container.remove();
    };
    act(() => root.render(<App />));
    await settle();
    const link = [...container.querySelectorAll("button")].find(
      (each) => each.textContent === "Game data",
    )!;
    act(() => link.click());
    await settle();
    // jsdom lays nothing out, so we say what is under the drop.
    document.elementFromPoint = () => container.querySelector(".game-data h1");

    await act(async () => shell.drop(["/Games/Sonic.md"], { x: 400, y: 300 }));
    await settle();

    expect(shell.identified).toEqual([]);
  });
});
