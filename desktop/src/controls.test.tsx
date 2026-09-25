import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";
import { ControlsEditor, emptyControls } from "./controls";
import registry from "../controls.json";
(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

// A console that uses the 16-control retropad fallback. ps1 has a separate
// illustrated profile.
function renderEditor(system = "atari2600") {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  function Example() {
    const [value, setValue] = useState(emptyControls);
    return <ControlsEditor system={system} value={value} onChange={setValue} />;
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

  it("captures a row keyboard binding directly without opening hidden panels", () => {
    const { container, cleanup } = renderEditor();
    try {
      expect(container.querySelector("svg.controller-scene")).toBeNull();
      expect(container.querySelector(".controls-grid")).toBeNull();
      expect(container.querySelector("details[open]")).toBeNull();
      expect(container.querySelectorAll("tbody tr")).toHaveLength(16);
      const l2 = bindingButton(container, "L2");
      expect(l2.textContent).toContain("W");
      click(l2);
      expect(l2.textContent).toMatch(/Press a key…\s*10/);
      expect(container.querySelector("details[open]")).toBeNull();
      press("Space", " ");
      expect(bindingButton(container, "L2").textContent).toContain("Space");
      expect(container.querySelector("details[open]")).toBeNull();
    } finally {
      cleanup();
    }
  });

  // Only Escape is reserved: it cancels here and opens the game's menu.
  it("binds Q and F like any other key", () => {
    const { container, cleanup } = renderEditor();
    try {
      click(bindingButton(container, "L2"));
      press("KeyQ", "q");
      expect(bindingButton(container, "L2").textContent).toContain("Q");
      click(bindingButton(container, "R2"));
      press("KeyF", "f");
      expect(bindingButton(container, "R2").textContent).toContain("F");
      expect(container.textContent).not.toContain("reserved");
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

  it("restores author keyboard defaults from a row without opening panels", () => {
    const { container, cleanup } = renderEditor();
    try {
      click(bindingButton(container, "L2"));
      press("Space", " ");
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

  it("keeps shared overrides when switching Mega Drive variants", () => {
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
      press("KeyZ", "z");
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
});
