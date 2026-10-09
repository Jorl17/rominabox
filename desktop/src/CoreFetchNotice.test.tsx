import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  CoreDownloadFailed,
  ExportBug,
  exportFailure,
  type ExportProgress,
} from "./bridge";
import {
  afterExport,
  afterProgress,
  CoreFetchNotice,
  type CoreNotice,
} from "./CoreFetchNotice";

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

function show(notice: CoreNotice, onBack = () => {}, onRetry = () => {}) {
  act(() =>
    root.render(
      <CoreFetchNotice notice={notice} onBack={onBack} onRetry={onRetry} />,
    ),
  );
  return container.querySelector(".pop-up") as HTMLElement;
}

const step = (message: string): ExportProgress => ({
  stage: "stage",
  fraction: 0.5,
  message,
});

describe("the export's core pop-up", () => {
  it("says how many cores it downloads", () => {
    const popup = show({ kind: "fetching", downloading: 1, updating: 0 });
    expect(popup.getAttribute("role")).toBe("status");
    expect(popup.textContent).toBe("Downloading 1 core");
    expect(popup.querySelector("button")).toBeNull();
  });

  it("says how many cores it updates", () => {
    expect(
      show({ kind: "fetching", downloading: 0, updating: 2 }).textContent,
    ).toBe("Updating 2 cores");
  });

  it("says both when it does both", () => {
    const popup = show({ kind: "fetching", downloading: 1, updating: 1 });
    expect(
      [...popup.querySelectorAll("p")].map((line) => line.textContent),
    ).toEqual(["Downloading 1 core", "Updating 1 core"]);
  });

  it("offers Go back then Retry when a core could not be downloaded", () => {
    const back = vi.fn();
    const retry = vi.fn();
    const popup = show(
      {
        kind: "failed",
        message: "The Dreamcast core could not be downloaded. Try again later.",
      },
      back,
      retry,
    );
    expect(popup.getAttribute("role")).toBe("alertdialog");
    expect(popup.querySelector("p")?.textContent).toBe(
      "The Dreamcast core could not be downloaded. Try again later.",
    );
    const buttons = [...popup.querySelectorAll("button")];
    expect(buttons.map((button) => button.textContent)).toEqual([
      "Go back",
      "Retry",
    ]);
    act(() => buttons[0].click());
    expect(back).toHaveBeenCalledTimes(1);
    expect(retry).not.toHaveBeenCalled();
    act(() => buttons[1].click());
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("does not appear when the export fetches nothing, and closes when it ends", () => {
    let notice: CoreNotice | null = null;
    for (const message of [
      "Checking export inputs",
      "Copying the game runtime",
    ])
      notice = afterProgress(notice, step(message));
    expect(notice).toBeNull();
    notice = afterProgress(notice, {
      ...step("Updating 1 core"),
      cores: { kind: "fetching", downloading: 0, updating: 1 },
    });
    expect(notice).toEqual({ kind: "fetching", downloading: 0, updating: 1 });
    notice = afterProgress(notice, step("Signing the local app"));
    expect(notice?.kind).toBe("fetching");
    expect(afterExport()).toBeNull();
    expect(afterExport(new Error("stage: something else"))).toBeNull();
  });

  it("stays open with the failure when a core could not be downloaded", () => {
    const failure = exportFailure({
      stage: "cores",
      sentence: "The Dreamcast core could not be downloaded. Try again later.",
    });
    expect(failure).toBeInstanceOf(CoreDownloadFailed);
    expect(afterExport(failure)).toEqual({
      kind: "failed",
      message: "The Dreamcast core could not be downloaded. Try again later.",
    });
  });

  it("shows every other export error as the exporter's sentence", () => {
    const sentence =
      "The app could not be written to its folder. Check that there is free space, then try again.";
    const other = exportFailure({ stage: "stage", sentence });
    expect(other).not.toBeInstanceOf(CoreDownloadFailed);
    expect((other as Error).message).toBe(sentence);
    expect(other).not.toBeInstanceOf(ExportBug);
  });

  it("keeps the details of a bug for the report", () => {
    const sentence =
      "The app could not be created, because of a bug in ROM-in-a-Box.";
    const details =
      "Menu design 'native' is declared but its package is missing at /kit/designs/native";
    const bug = exportFailure({ stage: "validate", sentence, bug: details });
    expect(bug).toBeInstanceOf(ExportBug);
    expect((bug as ExportBug).message).toBe(sentence);
    expect((bug as ExportBug).details).toBe(details);
  });
});
