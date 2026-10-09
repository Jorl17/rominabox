import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const asked = vi.hoisted(() => ({
  answer: {
    notice: [] as {
      heading: string | null;
      warnings: { text: string; detail: string }[];
    }[],
    existing: null as { app: string; folder: string } | null,
  },
}));

vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./bridge")>();
  return { ...actual, beforeExport: () => Promise.resolve(asked.answer) };
});

import { AppExists, exportFailure, type ExportRequest } from "./bridge";
import { askFirst, type Asking, CreateAppDialog } from "./CreateAppDialog";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  asked.answer = { notice: [], existing: null };
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

const MAC = {
  text: "The shader “Retro Glow” won’t load on a Mac. The game will run fine, but without the filter.",
  detail: "This is because Macs do not support this GLSL shader.",
};
const said = (text: string) => ({ text, detail: `Why ${text}` });

function show(
  asking: Asking,
  close: () => void = () => {},
  create: (replace: boolean) => void = () => {},
) {
  act(() =>
    root.render(
      <CreateAppDialog
        title="Sonic Advance"
        asking={asking}
        close={close}
        create={create}
      />,
    ),
  );
  return container.querySelector(".pop-up") as HTMLElement;
}

/** Each button by its words, or by its label for the help button, which has none. */
const buttons = (dialog: HTMLElement) =>
  [...dialog.querySelectorAll("button")].map(
    (button) => button.getAttribute("aria-label") ?? button.textContent,
  );

describe("an app with this name already in the folder", () => {
  it("is what the exporter's exists answer becomes", () => {
    const failure = exportFailure(inTheWay);
    expect(failure).toBeInstanceOf(AppExists);
    expect(failure).toMatchObject({
      app: "Super Mario Bros. 3",
      folder: "ROM-in-a-Box",
      message: "An app with this name already exists in ROM-in-a-Box.",
    });
  });

  it("alone, asks whether to replace it, with Cancel then Replace", () => {
    const existing = exportFailure(inTheWay) as AppExists;
    const dialog = show({ notice: [], existing });
    expect(dialog.getAttribute("role")).toBe("alertdialog");
    expect(dialog.querySelector("h2")?.textContent).toBe(
      "Replace “Super Mario Bros. 3”?",
    );
    expect(dialog.querySelector("p")?.textContent).toBe(
      "An app with this name already exists in ROM-in-a-Box.",
    );
    expect(buttons(dialog)).toEqual(["Cancel", "Replace"]);
    expect(document.activeElement?.textContent).toBe("Cancel");
  });

  it("does nothing but close on Cancel or Escape", () => {
    const close = vi.fn();
    const create = vi.fn();
    const existing = exportFailure(inTheWay) as AppExists;
    const dialog = show({ notice: [], existing }, close, create);
    act(() => (dialog.querySelectorAll("button")[0] as HTMLElement).click());
    act(() => {
      dialog.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
    });
    expect(close).toHaveBeenCalledTimes(2);
    expect(create).not.toHaveBeenCalled();
  });

  it("replaces on Replace", () => {
    const close = vi.fn();
    const create = vi.fn();
    const existing = exportFailure(inTheWay) as AppExists;
    const dialog = show({ notice: [], existing }, close, create);
    act(() => (dialog.querySelectorAll("button")[1] as HTMLElement).click());
    expect(close).toHaveBeenCalledTimes(1);
    expect(create).toHaveBeenCalledWith(true);
  });
});

describe("shaders that will not load", () => {
  it("on one platform are one part without a heading", () => {
    const create = vi.fn();
    const dialog = show(
      { notice: [{ heading: null, warnings: [MAC] }], existing: null },
      () => {},
      create,
    );
    expect(dialog.querySelector("h2")?.textContent).toBe(
      "Before we create “Sonic Advance”",
    );
    expect(dialog.querySelector("h3")).toBeNull();
    expect(
      [...dialog.querySelectorAll("p")].map((line) => line.textContent),
    ).toEqual([MAC.text]);
    expect(buttons(dialog)).toEqual(["Why", "Cancel", "Create anyway"]);
    act(() => (dialog.querySelectorAll("button")[2] as HTMLElement).click());
    expect(create).toHaveBeenCalledWith(false);
  });

  it("on both platforms are under their headings, before the app in the way", () => {
    const create = vi.fn();
    const dialog = show(
      {
        notice: [
          {
            heading: "On a Mac",
            warnings: [said("Mac one."), said("Mac two.")],
          },
          { heading: "On Windows", warnings: [said("Windows one.")] },
        ],
        existing: { app: "Sonic Advance", folder: "Games" },
      },
      () => {},
      create,
    );
    expect(
      [...dialog.querySelectorAll("h3")].map((heading) => heading.textContent),
    ).toEqual(["On a Mac", "On Windows"]);
    expect(
      [...dialog.querySelectorAll("p")].map((line) => line.textContent),
    ).toEqual([
      "Mac one.",
      "Mac two.",
      "Windows one.",
      "An app with this name already exists in Games. We’ll replace it.",
    ]);
    expect(buttons(dialog)).toEqual([
      "Why",
      "Why",
      "Why",
      "Cancel",
      "Create anyway",
    ]);
    act(() => (dialog.querySelectorAll("button")[4] as HTMLElement).click());
    expect(create).toHaveBeenCalledWith(true);
  });

  it("say why in the tooltip of the help button beside them", () => {
    const dialog = show({
      notice: [{ heading: null, warnings: [MAC] }],
      existing: null,
    });
    expect(document.querySelector("[role=tooltip]")).toBeNull();
    act(() => {
      dialog
        .querySelector(".help")!
        .dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    });
    expect(document.querySelector("[role=tooltip]")?.textContent).toBe(
      MAC.detail,
    );
  });
});

describe("on Create app", () => {
  const request = {} as ExportRequest;

  it("we create the app at once when there is nothing to ask", async () => {
    const ask = vi.fn();
    const create = vi.fn();
    await askFirst(request, ask, create);
    expect(ask).not.toHaveBeenCalled();
    expect(create).toHaveBeenCalledTimes(1);
  });

  it("we ask first when a shader will not load or an app is in the way", async () => {
    for (const answer of [
      { notice: [{ heading: null, warnings: [MAC] }], existing: null },
      { notice: [], existing: { app: "Sonic Advance", folder: "Games" } },
    ]) {
      asked.answer = answer;
      const ask = vi.fn();
      const create = vi.fn();
      await askFirst(request, ask, create);
      expect(ask).toHaveBeenCalledWith(answer);
      expect(create).not.toHaveBeenCalled();
    }
  });
});
