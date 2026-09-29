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
}));
vi.mock("./bridge", () => ({
  capturePadPosition: () => {
    const next = rust.presses.shift();
    return next ? Promise.resolve(next) : new Promise(() => {});
  },
  cancelPadCapture: () => Promise.resolve(),
  checkMenuControls: (menuControls: object) => {
    rust.checked.push(menuControls);
    return Promise.resolve(rust.refusals.shift() ?? null);
  },
}));
import declared from "../defaults.json";
import { MenuControlsEditor } from "./MenuControlsEditor";
import type { MenuControls } from "./menuControls";
(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let cleanup = () => {};
afterEach(() => {
  cleanup();
  rust.presses.length = rust.refusals.length = rust.checked.length = 0;
});

function show() {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const seen: { value: MenuControls } = { value: declared.menuControls };
  function Example() {
    const [value, setValue] = useState<MenuControls>(declared.menuControls);
    seen.value = value;
    return (
      <MenuControlsEditor value={value} onChange={setValue} busy={false} />
    );
  }
  act(() => root.render(<Example />));
  cleanup = () => {
    act(() => root.unmount());
    container.remove();
  };
  const row = (name: string) =>
    [...container.querySelectorAll(".menu-control-row")].find(
      (each) => each.querySelector(".menu-control-name")?.textContent === name,
    )!;
  const chips = (name: string) =>
    [...row(name).querySelectorAll(".menu-control-chip")].map(
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

describe("the builder's menu controls", () => {
  it("starts from the builder's defaults, in the words the game shows", () => {
    const { chips } = show();
    expect(chips("Menu")).toEqual(["Escape", "Home", "L3 + R3"]);
    expect(chips("Confirm")).toEqual(["Enter", "Bottom button"]);
    expect(chips("Back")).toEqual(["Escape", "Right button"]);
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
    expect(view.seen.value.menu).toEqual(declared.menuControls.menu);
  });

  it("keeps a change the game's rules refuse from happening, and says why", async () => {
    const view = show();
    rust.refusals.push({ kind: "noKey", action: "menu" });
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
      { ...declared.menuControls, menu: ["pad:home", "pad:l3+r3"] },
    ]);
    expect(view.said()).toBe("Menu needs a key.");
    expect(view.chips("Menu")).toEqual(["Escape", "Home", "L3 + R3"]);

    rust.refusals.push({
      kind: "shared",
      action: "confirm",
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
      ...view
        .row("Confirm")
        .querySelectorAll<HTMLElement>(".menu-control-chip"),
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
    expect(view.seen.value).toEqual(declared.menuControls);
  });
});
