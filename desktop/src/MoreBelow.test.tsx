import { act, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MoreBelow } from "./MoreBelow";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom does no layout, so here the area of the step returns a 400-point view,
// the content height we write on it, and the scroll position we set on it.
const scrolled = new WeakMap<HTMLElement, number>();
const LAYOUT = {
  clientHeight: {
    get(this: HTMLElement) {
      return this.tagName === "SECTION" ? 400 : 0;
    },
  },
  scrollHeight: {
    get(this: HTMLElement) {
      return Number(this.dataset.content ?? 0);
    },
  },
  scrollTop: {
    get(this: HTMLElement) {
      return scrolled.get(this) ?? 0;
    },
    set(this: HTMLElement, value: number) {
      scrolled.set(this, value);
    },
  },
};

function Step({ content }: { content: number }) {
  const area = useRef<HTMLElement>(null);
  return (
    <>
      <section ref={area} data-content={content} />
      <footer>
        <MoreBelow area={area} step={0} />
      </footer>
    </>
  );
}

let container: HTMLDivElement;
let root: Root;

async function show(content: number) {
  await act(async () => root.render(<Step content={content} />));
  await nextFrame();
}

// In MoreBelow we measure in the frame after a change.
async function nextFrame() {
  await act(
    () => new Promise<void>((done) => requestAnimationFrame(() => done())),
  );
}

const area = () => container.querySelector("section")!;
const arrow = () => container.querySelector<HTMLButtonElement>(".more-below")!;

beforeEach(() => {
  for (const [name, accessors] of Object.entries(LAYOUT)) {
    Object.defineProperty(HTMLElement.prototype, name, {
      configurable: true,
      ...accessors,
    });
  }
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  for (const name of Object.keys(LAYOUT)) {
    delete (HTMLElement.prototype as unknown as Record<string, unknown>)[name];
  }
});

describe("MoreBelow", () => {
  it("shows while the step continues below its view, and not once the end is in view", async () => {
    await show(900);
    expect(arrow().hidden).toBe(false);

    area().scrollTop = 500;
    area().dispatchEvent(new Event("scroll"));
    await nextFrame();
    expect(arrow().hidden).toBe(true);
  });

  it("stays hidden for a step that fits, and appears when its content grows", async () => {
    await show(380);
    expect(arrow().hidden).toBe(true);

    area().dataset.content = "1200";
    await nextFrame();
    expect(arrow().hidden).toBe(false);
  });

  it("scrolls the area down by most of its view", async () => {
    await show(900);
    const scrollBy = vi.fn();
    area().scrollBy = scrollBy;
    act(() => arrow().click());
    expect(scrollBy).toHaveBeenCalledWith({ top: 320, behavior: "smooth" });
  });
});
