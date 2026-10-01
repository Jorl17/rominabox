import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

// We read the controller and check the rules on the Rust side. Here a wait
// returns the next queued pad press, or goes on waiting when none is queued,
// and a rule check returns the next queued refusal, or no refusal.
const rust = vi.hoisted(() => ({
  presses: [] as string[],
  refusals: [] as object[],
  checked: [] as object[],
  games: [] as object[],
}));
vi.mock("./bridge", () => ({
  capturePadPosition: () => {
    const next = rust.presses.shift();
    return next ? Promise.resolve(next) : new Promise(() => {});
  },
  cancelPadCapture: () => Promise.resolve(),
  checkHotkeys: (hotkeys: object, system: string, controls: object) => {
    rust.checked.push(hotkeys);
    rust.games.push({ system, controls });
    return Promise.resolve(rust.refusals.shift() ?? null);
  },
}));
import declared from "../defaults.json";
import { HotkeysEditor } from "./HotkeysEditor";
import type { Hotkeys } from "./hotkeys";
(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let cleanup = () => {};
afterEach(() => {
  cleanup();
  rust.presses.length = rust.refusals.length = rust.checked.length = 0;
  rust.games.length = 0;
});

/** A Mega Drive game whose C is on F2. */
const game = {
  system: "megadrive",
  controls: { bindings: { a: { key: "f2" } } },
};

function show() {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const seen: { value: Hotkeys } = { value: declared.hotkeys };
  function Example() {
    const [value, setValue] = useState<Hotkeys>(declared.hotkeys);
    seen.value = value;
    return (
      <HotkeysEditor
        value={value}
        onChange={setValue}
        busy={false}
        game={game}
      />
    );
  }
  act(() => root.render(<Example />));
  cleanup = () => {
    act(() => root.unmount());
    container.remove();
  };
  const row = (name: string) =>
    [...container.querySelectorAll(".hotkey-row")].find(
      (each) => each.querySelector(".hotkey-name")?.textContent === name,
    )!;
  const chips = (name: string) =>
    [...row(name).querySelectorAll(".hotkey-chip")].map(
      (chip) => chip.textContent,
    );
  const said = () => container.querySelector("[role=status]")?.textContent;
  return { container, row, chips, said, seen };
}

async function settle() {
  await act(async () => {
    for (let turn = 0; turn < 5; turn += 1) await Promise.resolve();
  });
}

function press(code: string, key = code) {
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key, code, bubbles: true }),
    );
  });
}

describe("the builder's hotkeys", () => {
  it("starts from the builder's defaults, in the words the game shows", () => {
    const { chips } = show();
    expect(chips("Menu")).toEqual(["Escape", "Home", "L3 + R3"]);
    expect(chips("Confirm")).toEqual(["Enter", "Bottom button"]);
    expect(chips("Back")).toEqual(["Escape", "Right button"]);
    expect(chips("Quick save")).toEqual(["F2"]);
    expect(chips("Quick load")).toEqual(["F4"]);
    expect(chips("Previous slot")).toEqual(["F6"]);
    expect(chips("Next slot")).toEqual(["F7"]);
  });

  it("adds a key or a pad button pressed after +, and says no to a stick", async () => {
    const view = show();
    act(() =>
      view
        .row("Confirm")
        .querySelector<HTMLButtonElement>("[aria-label='Add to Confirm']")!
        .click(),
    );
    press("Space", " ");
    await settle();
    expect(view.chips("Confirm")).toEqual(["Enter", "Bottom button", "Space"]);
    expect(view.seen.value.confirm).toEqual([
      "key:enter",
      "pad:b",
      "key:space",
    ]);

    rust.presses.push("r3");
    act(() =>
      view
        .row("Back")
        .querySelector<HTMLButtonElement>("[aria-label='Add to Back']")!
        .click(),
    );
    await settle();
    expect(view.seen.value.back).toEqual(["key:escape", "pad:a", "pad:r3"]);

    rust.presses.push("l_x_plus");
    act(() =>
      view
        .row("Menu")
        .querySelector<HTMLButtonElement>("[aria-label='Add to Menu']")!
        .click(),
    );
    await settle();
    expect(view.said()).toBe("Use a key or a pad button.");
    expect(view.seen.value.menu).toEqual(declared.hotkeys.menu);
  });

  it("keeps a change the game's rules refuse from happening, and says why", async () => {
    const view = show();
    rust.refusals.push({ kind: "noKey", hotkey: "menu" });
    act(() =>
      view
        .row("Menu")
        .querySelector<HTMLButtonElement>(
          "[aria-label='Remove Escape from Menu']",
        )!
        .click(),
    );
    await settle();
    expect(rust.checked).toEqual([
      { ...declared.hotkeys, menu: ["pad:home", "pad:l3+r3"] },
    ]);
    expect(view.said()).toBe("Menu needs a key.");
    expect(view.chips("Menu")).toEqual(["Escape", "Home", "L3 + R3"]);

    rust.refusals.push({
      kind: "shared",
      hotkey: "confirm",
      other: "back",
      binding: "key:enter",
    });
    act(() =>
      view
        .row("Back")
        .querySelector<HTMLButtonElement>("[aria-label='Add to Back']")!
        .click(),
    );
    press("Enter");
    await settle();
    expect(view.said()).toBe("Enter is already Confirm's.");
    expect(view.chips("Back")).toEqual(["Escape", "Right button"]);
  });

  it("removes a binding with its ×, not with its words", async () => {
    const view = show();
    const enter = [
      ...view.row("Confirm").querySelectorAll<HTMLElement>(".hotkey-chip"),
    ].find((chip) => chip.textContent === "Enter")!;
    act(() => enter.click());
    await settle();
    expect(rust.checked).toEqual([]);
    expect(view.chips("Confirm")).toEqual(["Enter", "Bottom button"]);
    act(() => enter.querySelector("button")!.click());
    await settle();
    expect(view.chips("Confirm")).toEqual(["Bottom button"]);
  });

  it("goes back to the builder's defaults", async () => {
    const view = show();
    act(() =>
      view
        .row("Confirm")
        .querySelector<HTMLButtonElement>(
          "[aria-label='Remove Enter from Confirm']",
        )!
        .click(),
    );
    await settle();
    expect(view.chips("Confirm")).toEqual(["Bottom button"]);
    const reset = [...view.container.querySelectorAll("button")].find(
      (button) => button.textContent === "Reset to defaults",
    )!;
    act(() => reset.click());
    expect(view.seen.value).toEqual(declared.hotkeys);
  });

  it("leaves a hotkey that acts while the game plays with no binding", async () => {
    const view = show();
    act(() =>
      view
        .row("Quick save")
        .querySelector<HTMLButtonElement>(
          "[aria-label='Remove F2 from Quick save']",
        )!
        .click(),
    );
    await settle();
    expect(view.chips("Quick save")).toEqual([]);
    expect(view.seen.value["quick-save"]).toEqual([]);
    expect(view.said()).toBe("");
  });

  it("asks with the game's controls, and says when a key is the game's", async () => {
    const view = show();
    rust.refusals.push({
      kind: "gameKey",
      hotkey: "next-slot",
      binding: "key:f2",
      control: "a",
      label: "C",
    });
    act(() =>
      view
        .row("Next slot")
        .querySelector<HTMLButtonElement>("[aria-label='Add to Next slot']")!
        .click(),
    );
    press("F2", "F2");
    await settle();
    expect(rust.games).toEqual([game]);
    expect(rust.checked).toEqual([
      { ...declared.hotkeys, "next-slot": ["key:f7", "key:f2"] },
    ]);
    expect(view.said()).toBe("F2 is the game's key for C.");
    expect(view.chips("Next slot")).toEqual(["F7"]);
  });
});
