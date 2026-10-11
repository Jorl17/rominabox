import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type * as bridge from "./bridge";
import { BuilderUpdateButton } from "./BuilderUpdateButton";
import { setChecksForUpdates } from "./updatePreference";

/** What the stand-in for the builder answers, and what we asked of it. */
const builder = {
  update: null as bridge.BuilderUpdate | null,
  install: async (
    onProgress: (received: number, total: number | null) => void,
  ) => {
    onProgress(50, 200);
  },
  calls: [] as string[],
};
vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof bridge>();
  return {
    ...actual,
    checkBuilderUpdate: async () => {
      builder.calls.push("check");
      return builder.update;
    },
    installBuilderUpdate: (
      onProgress: (received: number, total: number | null) => void,
    ) => {
      builder.calls.push("install");
      return builder.install(onProgress);
    },
    openWebsite: async () => {
      builder.calls.push("website");
    },
  };
});

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  builder.update = null;
  builder.install = async (onProgress) => onProgress(50, 200);
  builder.calls = [];
  localStorage.clear();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

async function shown() {
  await act(async () => root.render(<BuilderUpdateButton />));
}
const button = () => container.querySelector<HTMLButtonElement>("button");

describe("the update button of the builder", () => {
  it("shows nothing when there is no newer builder", async () => {
    await shown();
    expect(builder.calls).toEqual(["check"]);
    expect(container.textContent).toBe("");
  });

  it("offers a newer builder, installs it and shows how far the download is", async () => {
    builder.update = { version: "0.5.1", notes: "Fixes.", installable: true };
    // The builder starts again once the update is installed, so the promise
    // of the install never settles while we look.
    builder.install = (onProgress) => {
      onProgress(50, 200);
      return new Promise(() => {});
    };
    await shown();
    expect(button()?.textContent).toBe("Update to 0.5.1");
    await act(async () => button()!.click());
    expect(builder.calls).toEqual(["check", "install"]);
    expect(button()?.textContent).toBe("Updating… 25%");
    expect(button()?.disabled).toBe(true);
  });

  it("says why it could not install, and lets the person try again", async () => {
    builder.update = { version: "0.5.1", notes: null, installable: true };
    builder.install = async () => {
      throw "Wait for the export to finish, then update.";
    };
    await shown();
    await act(async () => button()!.click());
    expect(container.textContent).toContain(
      "Wait for the export to finish, then update.",
    );
    expect(button()?.textContent).toBe("Update to 0.5.1");
    expect(button()?.disabled).toBe(false);
  });

  it("opens the website when the builder cannot be replaced where it is", async () => {
    builder.update = { version: "0.5.1", notes: null, installable: false };
    await shown();
    await act(async () => button()!.click());
    expect(builder.calls).toEqual(["check", "website"]);
  });

  it("does not look for updates when the person turned them off", async () => {
    setChecksForUpdates(false);
    builder.update = { version: "0.5.1", notes: null, installable: true };
    await shown();
    expect(builder.calls).toEqual([]);
    expect(container.textContent).toBe("");
  });
});
