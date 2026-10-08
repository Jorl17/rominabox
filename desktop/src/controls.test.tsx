import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

// We read the controller on the Rust side. Here we answer as a pad would:
// each wait returns the next queued press, or goes on waiting with none.
// We check the hotkey rules there too. Here each check returns the next
// queued refusal, or no refusal with none queued.
const pad = vi.hoisted(() => ({
  presses: [] as string[],
  asked: 0,
  refusals: [] as object[],
  checked: [] as object[],
}));
vi.mock("./bridge", () => ({
  capturePadPosition: () => {
    pad.asked += 1;
    const next = pad.presses.shift();
    return next ? Promise.resolve(next) : new Promise(() => {});
  },
  cancelPadCapture: () => Promise.resolve(),
  checkHotkeys: (hotkeys: object, system: string, controls: object) => {
    pad.checked.push({ hotkeys, system, controls });
    return Promise.resolve(pad.refusals.shift() ?? null);
  },
}));
import { ControlsEditor, emptyControls } from "./controls";
import { defaultHotkeys } from "./hotkeys";
import registry from "../controls.json";
(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;
afterEach(() => {
  pad.refusals.length = pad.checked.length = 0;
});

// A console that uses the 16-control retropad fallback. ps1 has a separate
// illustrated profile.
function renderEditor(system = "atari7800") {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  function Example() {
    const [value, setValue] = useState(emptyControls);
    return (
      <ControlsEditor
        system={system}
        value={value}
        onChange={setValue}
        hotkeys={defaultHotkeys}
      />
    );
  }
  act(() => root.render(<Example />));
  return {
    container,
    cleanup() {
      act(() => root.unmount());
      container.remove();
    },
  };
}

/** Let the answers for a waiting capture arrive. */
async function settle() {
  await act(async () => {
    for (let tick = 0; tick < 20; tick += 1) await Promise.resolve();
  });
}

function buttonNamed(container: HTMLElement, name: string) {
  const found = [...container.querySelectorAll("button")].find(
    (button) => button.textContent?.trim() === name,
  );
  expect(found, name).toBeTruthy();
  return found!;
}

function row(container: HTMLElement, button: string) {
  const match = [...container.querySelectorAll("tbody tr")].find(
    (candidate) =>
      candidate.querySelector("th")?.textContent?.trim() === button,
  );
  expect(match, button).toBeTruthy();
  return match!;
}

function bindingButton(container: HTMLElement, button: string) {
  return row(container, button).querySelector(
    ".binding-button",
  ) as HTMLButtonElement;
}

function actionInput(container: HTMLElement, button: string) {
  return row(container, button).querySelector("input") as HTMLInputElement;
}

function click(element: HTMLElement) {
  act(() => element.click());
}

function press(code: string, key = code) {
  act(() =>
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key, code, bubbles: true }),
    ),
  );
}

/** A key pressed while we wait for a control, and the hotkey check result. */
async function pressed(code: string, key = code) {
  press(code, key);
  await settle();
}

function padSelect(container: HTMLElement, button: string) {
  return row(container, button).querySelector(
    `[aria-label="${button} pad"]`,
  ) as HTMLSelectElement | null;
}

function choose(select: HTMLSelectElement, value: string) {
  act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

function enterText(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )?.set;
  act(() => {
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("controller authoring", () => {
  it("calls the author-facing column action label", () => {
    const { container, cleanup } = renderEditor();
    try {
      const headers = [...container.querySelectorAll("thead th")].map((cell) =>
        cell.textContent?.trim(),
      );
      expect(headers).toContain("Action label");
    } finally {
      cleanup();
    }
  });

  it("keeps shared keyboard defaults consistent across Mega Drive variants", () => {
    const three = registry.profiles.find((p) => p.id === "megadrive")!;
    const six = registry.profiles.find((p) => p.id === "megadrive6")!;
    for (const input of three.controls)
      expect(
        six.controls.find((c) => c.id === input.id)?.key,
        input.label,
      ).toBe(input.key);
  });

  it("captures a row keyboard binding directly without opening hidden panels", async () => {
    const { container, cleanup } = renderEditor();
    try {
      expect(container.querySelector("svg.controller-scene")).toBeNull();
      expect(container.querySelector(".controls-grid")).toBeNull();
      expect(container.querySelector("details[open]")).toBeNull();
      expect(container.querySelectorAll("tbody tr")).toHaveLength(16);
      const l2 = bindingButton(container, "L2");
      expect(l2.textContent).toContain("W");
      click(l2);
      expect(l2.textContent).toMatch(/Press an input…\s*10/);
      expect(container.querySelector("details[open]")).toBeNull();
      await pressed("Space", " ");
      expect(bindingButton(container, "L2").textContent).toContain("Space");
      expect(container.querySelector("details[open]")).toBeNull();
    } finally {
      cleanup();
    }
  });

  // Only Escape is reserved: it cancels here and opens the game's menu.
  it("binds Q and F like any other key", async () => {
    const { container, cleanup } = renderEditor();
    try {
      click(bindingButton(container, "L2"));
      await pressed("KeyQ", "q");
      expect(bindingButton(container, "L2").textContent).toContain("Q");
      click(bindingButton(container, "R2"));
      await pressed("KeyF", "f");
      expect(bindingButton(container, "R2").textContent).toContain("F");
      expect(container.textContent).not.toContain("reserved");
    } finally {
      cleanup();
    }
  });

  // We store RetroArch names (shift, num7), and show the author the key.
  it("words a captured modifier and number as the keys that were pressed", async () => {
    const { container, cleanup } = renderEditor();
    const shown = (button: string) =>
      bindingButton(container, button).querySelector(".binding-key")
        ?.textContent;
    try {
      expect(shown("L3")).toBe("1");
      click(bindingButton(container, "L2"));
      await pressed("ShiftLeft", "Shift");
      expect(shown("L2")).toBe("Left Shift");
      click(bindingButton(container, "R2"));
      await pressed("Digit7", "7");
      expect(shown("R2")).toBe("7");
    } finally {
      cleanup();
    }
  });

  it("leaves the original binding unchanged when capture is cancelled", () => {
    const { container, cleanup } = renderEditor();
    try {
      click(bindingButton(container, "L2"));
      press("Escape", "Escape");
      expect(bindingButton(container, "L2").textContent).toContain("W");
      expect(document.activeElement).toBe(bindingButton(container, "L2"));
      click(bindingButton(container, "L2"));
      const cancel = [...container.querySelectorAll("button")].find(
        (button) => button.textContent?.trim() === "Cancel",
      );
      expect(cancel).toBeTruthy();
      click(cancel!);
      expect(bindingButton(container, "L2").textContent).toContain("W");
    } finally {
      cleanup();
    }
  });

  it("restores author keyboard defaults from a row without opening panels", async () => {
    const { container, cleanup } = renderEditor();
    try {
      click(bindingButton(container, "L2"));
      await pressed("Space", " ");
      expect(bindingButton(container, "L2").textContent).toContain("Space");
      const reset = [...container.querySelectorAll("button")].find(
        (button) => button.textContent?.trim() === "Reset to defaults",
      );
      expect(reset).toBeTruthy();
      click(reset!);
      expect(bindingButton(container, "L2").textContent).toContain("W");
      expect(container.querySelector("details[open]")).toBeNull();
    } finally {
      cleanup();
    }
  });

  it("edits action labels independently without opening hidden panels", () => {
    const { container, cleanup } = renderEditor();
    try {
      const l2 = actionInput(container, "L2");
      const r2 = actionInput(container, "R2");
      expect(l2.placeholder).toBe("L2");
      expect(r2.placeholder).toBe("R2");
      expect(l2.value).toBe("");
      enterText(l2, "Grenade");
      enterText(r2, "Strafe");
      expect(l2.value).toBe("Grenade");
      expect(r2.value).toBe("Strafe");
      expect(container.querySelector("details[open]")).toBeNull();
      expect(container.textContent).not.toContain("Device bindings");
    } finally {
      cleanup();
    }
  });

  it("keeps shared overrides when switching Mega Drive variants", async () => {
    const { container, cleanup } = renderEditor("megadrive");
    try {
      const variant = container.querySelector(
        '[aria-label="Controller variant"]',
      ) as HTMLSelectElement;
      expect([...variant.options].map((option) => option.textContent)).toEqual([
        "Mega Drive · 3 buttons",
        "Mega Drive · 6 buttons",
      ]);
      enterText(actionInput(container, "A"), "Jump");
      click(bindingButton(container, "Start"));
      await pressed("KeyZ", "z");
      expect(container.textContent).toMatch(/also controls/);
      const confirm = [...container.querySelectorAll("button")].find(
        (button) => button.textContent?.trim() === "Use for both",
      );
      click(confirm!);
      act(() => {
        variant.value = "megadrive6";
        variant.dispatchEvent(new Event("change", { bubbles: true }));
      });
      expect(actionInput(container, "A").value).toBe("Jump");
      expect(bindingButton(container, "Start").textContent).toContain("Z");
      expect(row(container, "Mode")).toBeTruthy();
    } finally {
      cleanup();
    }
  });

  it("moves a control onto another's pad position by swapping them", async () => {
    const { container, cleanup } = renderEditor("megadrive");
    try {
      choose(padSelect(container, "C")!, "b");
      await settle();
      expect(padSelect(container, "C")!.value).toBe("b");
      expect(padSelect(container, "B")!.value).toBe("a");
      expect(container.textContent).toContain("Pad updated.");
    } finally {
      cleanup();
    }
  });

  it("refuses a position another offered pad's control keeps", () => {
    const { container, cleanup } = renderEditor("megadrive");
    try {
      // The top button is Y on the six-button pad, at the same place in the game.
      choose(padSelect(container, "C")!, "x");
      expect(padSelect(container, "C")!.value).toBe("a");
      expect(container.textContent).toContain("Top button is taken.");
    } finally {
      cleanup();
    }
  });

  // In the game's capture we accept a wheel as well as a button, so the
  // author can set one. The words are from the game (mouse_buttons.inc).
  it("offers every mouse button the game declares, wheels included", () => {
    const { container, cleanup } = renderEditor("megadrive");
    try {
      const mouse = row(container, "C").querySelector(
        "details select:not([aria-label])",
      ) as HTMLSelectElement;
      expect([...mouse.options].map((option) => option.textContent)).toEqual([
        "None",
        "Left",
        "Right",
        "Middle",
        "Button 4",
        "Button 5",
        "Wheel up",
        "Wheel down",
        "Wheel left",
        "Wheel right",
      ]);
      choose(mouse, "wu");
      expect(mouse.value).toBe("wu");
    } finally {
      cleanup();
    }
  });

  // With one Bind we listen to the keyboard and the controllers at once.
  it("moves a control to the position pressed on a controller", async () => {
    // R2: no Mega Drive pad uses it.
    pad.presses = ["r2"];
    const { container, cleanup } = renderEditor("megadrive");
    try {
      click(bindingButton(container, "C"));
      await settle();
      expect(padSelect(container, "C")!.value).toBe("r2");
      expect(
        bindingButton(container, "C").querySelector(".binding-pad")
          ?.textContent,
      ).toBe("R2");
      expect(bindingButton(container, "C").textContent).toContain("C");
      expect(container.textContent).toContain("Binding updated.");
    } finally {
      pad.presses = [];
      cleanup();
    }
  });

  it("swaps with the control whose position was pressed, and refuses one another pad keeps", async () => {
    pad.presses = ["b", "x"];
    const { container, cleanup } = renderEditor("megadrive");
    try {
      click(bindingButton(container, "C"));
      await settle();
      expect(padSelect(container, "C")!.value).toBe("b");
      expect(padSelect(container, "B")!.value).toBe("a");
      click(bindingButton(container, "A"));
      await settle();
      expect(container.textContent).toContain(
        "Top button is taken. Binding unchanged.",
      );
      expect(padSelect(container, "A")!.value).toBe("y");
    } finally {
      pad.presses = [];
      cleanup();
    }
  });

  function stickMember(container: HTMLElement, label: string) {
    return row(container, label).querySelector(".binding-text")!;
  }

  // We bind a stick as in the game: its directions in order, each with one
  // press of a key or of the pad, with a mark on the one we wait for.
  it("binds a stick direction by direction, with keys", async () => {
    const { container, cleanup } = renderEditor("ps1");
    try {
      expect(padSelect(container, "Left stick up")).toBeTruthy();
      click(buttonNamed(container, "Bind stick"));
      expect(row(container, "Left stick up").className).toContain("capturing");
      expect(stickMember(container, "Left stick up").textContent).toMatch(
        /Press an input…\s*10/,
      );
      await pressed("KeyY", "y");
      expect(row(container, "Left stick up").className).not.toContain(
        "capturing",
      );
      expect(row(container, "Left stick right").className).toContain(
        "capturing",
      );
      await pressed("KeyM", "m");
      await pressed("KeyN", "n");
      await pressed("KeyH", "h");
      expect(row(container, "Left stick press").className).toContain(
        "capturing",
      );
      await pressed("KeyU", "u");
      expect(container.textContent).toContain("Binding updated.");
      expect(stickMember(container, "Left stick up").textContent).toBe(
        "YLeft stick up",
      );
      expect(stickMember(container, "Left stick left").textContent).toBe(
        "HLeft stick left",
      );
      expect(stickMember(container, "Left stick press").textContent).toBe(
        "UL3",
      );
      expect(container.querySelector(".binding-stops")).toBeNull();
    } finally {
      cleanup();
    }
  });

  it("binds a stick onto the pad's d-pad, which takes the stick's place", async () => {
    pad.presses = ["up", "right", "down", "left", "l3"];
    const { container, cleanup } = renderEditor("ps1");
    try {
      click(buttonNamed(container, "Bind stick"));
      await settle();
      expect(container.textContent).toContain("Binding updated.");
      expect(stickMember(container, "Left stick up").textContent).toBe(
        "TD-pad up",
      );
      expect(stickMember(container, "Left stick left").textContent).toBe(
        "VD-pad left",
      );
      expect(padSelect(container, "Up")!.value).toBe("l_y_minus");
      expect(padSelect(container, "Left")!.value).toBe("l_x_minus");
      expect(padSelect(container, "Left stick press")!.value).toBe("l3");
      expect(container.querySelector(".binding-stops")).toBeNull();
      expect(pad.asked).toBeGreaterThanOrEqual(5);
    } finally {
      pad.presses = [];
      cleanup();
    }
  });

  it("stops a stick partway on Escape, saying what was saved and what stops working", async () => {
    pad.presses = ["up"];
    const { container, cleanup } = renderEditor("ps1");
    try {
      click(buttonNamed(container, "Bind stick"));
      await settle();
      expect(row(container, "Left stick right").className).toContain(
        "capturing",
      );
      press("Escape");
      expect(container.textContent).toContain("Saved Left stick up.");
      expect(stickMember(container, "Left stick up").textContent).toBe(
        "TD-pad up",
      );
      expect(stickMember(container, "Left stick right").textContent).toBe(
        "BLeft stick right",
      );
      expect(
        row(container, "Left stick down").querySelector(".binding-stops")
          ?.textContent,
      ).toBe("Left stick down stops working while Left stick up is moved.");
      expect(document.activeElement).toBe(buttonNamed(container, "Bind stick"));
    } finally {
      pad.presses = [];
      cleanup();
    }
  });

  it("asks before a stick's key takes another control's, then goes on", async () => {
    const { container, cleanup } = renderEditor("ps1");
    try {
      click(buttonNamed(container, "Bind stick"));
      // Z is Cross's key.
      await pressed("KeyZ", "z");
      expect(container.textContent).toContain("Z also controls Cross.");
      click(buttonNamed(container, "Use for both"));
      expect(stickMember(container, "Left stick up").textContent).toBe(
        "ZLeft stick up",
      );
      expect(row(container, "Left stick right").className).toContain(
        "capturing",
      );
      press("Escape");
      expect(container.textContent).toContain("Saved Left stick up.");
    } finally {
      cleanup();
    }
  });
});

// A hotkey that works while the game runs must not use a key or pad button of
// the game. We reject one in the export, when saving a project and in the
// Hotkeys section, and also when a control is bound to it.
describe("the hotkeys' inputs", () => {
  const keyOf = (container: HTMLElement, button: string) =>
    bindingButton(container, button).querySelector(".binding-key")?.textContent;

  it("refuses a key a hotkey that acts while the game plays holds", async () => {
    pad.refusals.push({
      kind: "gameInput",
      hotkey: "quick-save",
      binding: "key:f2",
      control: "a",
      label: "C",
    });
    const { container, cleanup } = renderEditor("megadrive");
    try {
      click(bindingButton(container, "C"));
      press("F2", "F2");
      await settle();
      expect(keyOf(container, "C")).toBe("C");
      expect(container.textContent).toContain(
        "F2 is already Quick save's. Binding unchanged.",
      );
      expect(pad.checked).toEqual([
        {
          hotkeys: defaultHotkeys,
          system: "megadrive",
          controls: { bindings: { a: { key: "f2" } } },
        },
      ]);
    } finally {
      cleanup();
    }
  });

  // We reject a stick moved between two of its directions in the export, and
  // the Hotkeys section has separate rules. Neither stops a key here.
  it("takes a key whatever else the hotkeys' check says", async () => {
    pad.refusals.push({ kind: "controls", message: "A stick is half moved." });
    const { container, cleanup } = renderEditor("megadrive");
    try {
      click(bindingButton(container, "C"));
      await pressed("KeyQ", "q");
      expect(keyOf(container, "C")).toBe("Q");
      expect(pad.checked).toHaveLength(1);
    } finally {
      cleanup();
    }
  });

  it("refuses a pad button a hotkey that acts while the game plays holds, pressed or chosen", async () => {
    const r2 = {
      kind: "gameInput",
      hotkey: "quick-save",
      binding: "pad:r2",
      control: "a",
      label: "C",
    };
    pad.refusals.push(r2, r2);
    pad.presses = ["r2"];
    const { container, cleanup } = renderEditor("megadrive");
    try {
      click(bindingButton(container, "C"));
      await settle();
      expect(padSelect(container, "C")!.value).toBe("a");
      expect(container.textContent).toContain(
        "R2 is already Quick save's. Binding unchanged.",
      );
      choose(padSelect(container, "C")!, "r2");
      await settle();
      expect(padSelect(container, "C")!.value).toBe("a");
      expect(container.textContent).toContain("R2 is already Quick save's.");
      expect(pad.checked).toHaveLength(2);
    } finally {
      pad.presses = [];
      cleanup();
    }
  });
});
