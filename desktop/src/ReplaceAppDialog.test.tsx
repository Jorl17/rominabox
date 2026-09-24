import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AppExists, exportFailure } from "./bridge";
import { ReplaceAppDialog } from "./ReplaceAppDialog";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** The exporter error when an app is in the way (`AuthorError`). */
const inTheWay = {
  stage: "exists",
  sentence: "An app with this name already exists in ROM-in-a-Box.",
  existing: { name: "Super Mario Bros. 3", folder: "ROM-in-a-Box" },
};

function show(onCancel = () => {}, onReplace = () => {}) {
  const existing = exportFailure(inTheWay) as AppExists;
  act(() =>
    root.render(
      <ReplaceAppDialog
        existing={existing}
        onCancel={onCancel}
        onReplace={onReplace}
      />,
    ),
  );
  return container.querySelector(".pop-up") as HTMLElement;
}

describe("replacing an app that is already there", () => {
  it("is what the exporter's exists answer becomes", () => {
    const failure = exportFailure(inTheWay);
    expect(failure).toBeInstanceOf(AppExists);
    expect(failure).toMatchObject({
      app: "Super Mario Bros. 3",
      folder: "ROM-in-a-Box",
      message: "An app with this name already exists in ROM-in-a-Box.",
    });
  });

  it("asks, naming the app and its folder, with Cancel then Replace", () => {
    const dialog = show();
    expect(dialog.getAttribute("role")).toBe("alertdialog");
    expect(dialog.querySelector("h2")?.textContent).toBe(
      "Replace “Super Mario Bros. 3”?",
    );
    expect(dialog.querySelector("p")?.textContent).toBe(
      "An app with this name already exists in ROM-in-a-Box.",
    );
    expect(
      [...dialog.querySelectorAll("button")].map(
        (button) => button.textContent,
      ),
    ).toEqual(["Cancel", "Replace"]);
    expect(document.activeElement?.textContent).toBe("Cancel");
  });

  it("does nothing but close on Cancel", () => {
    const cancel = vi.fn();
    const replace = vi.fn();
    const dialog = show(cancel, replace);
    act(() => (dialog.querySelectorAll("button")[0] as HTMLElement).click());
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(replace).not.toHaveBeenCalled();

    act(() => {
      dialog.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
    });
    expect(cancel).toHaveBeenCalledTimes(2);
    expect(replace).not.toHaveBeenCalled();
  });

  it("replaces on Replace", () => {
    const cancel = vi.fn();
    const replace = vi.fn();
    const dialog = show(cancel, replace);
    act(() => (dialog.querySelectorAll("button")[1] as HTMLElement).click());
    expect(replace).toHaveBeenCalledTimes(1);
    expect(cancel).not.toHaveBeenCalled();
  });
});
