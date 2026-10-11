import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ActionMenu } from "./ActionMenu";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom does no layout. Here a scrolling area shows the band from 0 to 500,
// the ellipsis sits at `button` in it, and its list is 200 tall, under the
// ellipsis unless it is above it.
const LIST = 200;
let button = 0;
function box(top: number, height: number): DOMRect {
  return { top, bottom: top + height, height } as DOMRect;
}
function layout(this: HTMLElement): DOMRect {
  if (this.classList.contains("scrolls")) return box(0, 500);
  if (this.classList.contains("action-menu")) return box(button, 32);
  if (this.classList.contains("action-menu-list")) {
    return this.classList.contains("above")
      ? box(button - 6 - LIST, LIST)
      : box(button + 32 + 6, LIST);
  }
  return box(0, 0);
}

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    layout,
  );
  container = document.createElement("div");
  container.className = "scrolls";
  container.style.overflowY = "auto";
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

/** The list of the menu, opened with its ellipsis at `at`. */
function openedAt(at: number): HTMLElement {
  button = at;
  act(() =>
    root.render(
      <ActionMenu
        label="More"
        items={[{ label: "Update core", onSelect: () => {} }]}
      />,
    ),
  );
  act(() => container.querySelector<HTMLButtonElement>("button")!.click());
  return container.querySelector<HTMLElement>(".action-menu-list")!;
}

describe("an action menu", () => {
  it("opens under the ellipsis where its list fits", () => {
    expect(openedAt(40).classList.contains("above")).toBe(false);
  });

  it("opens above the ellipsis near the bottom of a scrolling area, where its list would add a scroll", () => {
    expect(openedAt(420).classList.contains("above")).toBe(true);
  });
});
