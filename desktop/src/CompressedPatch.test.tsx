import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type * as bridge from "./bridge";

// The answer from the exporter for a compressed disc with a patch beside it:
// we ask until the author includes the patch, and stop once it is left out.
vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof bridge>();
  return {
    ...actual,
    travelingFiles: async (
      _path: string,
      _system: string,
      files?: bridge.GameFiles,
    ): Promise<bridge.Traveling> => {
      const leftOut = files?.leftOut.includes("Director's Cut.xdelta");
      return {
        entry: "/games/Game.chd",
        files: [{ name: "Game.chd", role: { kind: "game" } }],
        patches: leftOut ? [] : ["Director's Cut.xdelta"],
        refused: [],
        added: [],
        compressed: leftOut
          ? null
          : {
              game: "Game.chd",
              patches: ["Director's Cut.xdelta"],
              withoutBytes: 412 * 1024 * 1024,
              withBytes: 734 * 1024 * 1024,
              included: Boolean(files?.decompress),
            },
      };
    },
  };
});

const { CompressedPatchQuestion } = await import("./CompressedPatch");

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

/** The files on the details step, which we replace in `onChange`. */
async function show(onChange = vi.fn()) {
  let files: bridge.GameFiles = { leftOut: [], added: [] };
  const render = () =>
    root.render(
      <CompressedPatchQuestion
        entry="/games/Game.chd"
        system="ps1"
        files={files}
        onChange={(next, names) => {
          files = next;
          onChange(next, names);
          render();
        }}
        onError={(error) => {
          throw error;
        }}
      />,
    );
  await act(async () => render());
  return onChange;
}

const popUp = () => container.querySelector(".pop-up") as HTMLElement | null;
const button = (name: string) =>
  [...container.querySelectorAll("button")].find(
    (candidate) => candidate.textContent === name,
  ) as HTMLElement;

describe("a patch for a compressed disc", () => {
  it("asks, naming the patch and the game, with the size either way", async () => {
    await show();
    expect(popUp()?.querySelector("h2")?.textContent).toBe("Apply the patch?");
    expect(popUp()?.textContent).toContain(
      "“Director's Cut.xdelta” changes “Game.chd”, which is compressed.",
    );
    expect(popUp()?.querySelector(".pop-up-sizes")?.textContent).toBe(
      "Without the patch: 412 MB · With the patch: 734 MB",
    );
    expect(
      [...popUp()!.querySelectorAll("button")].map((each) => each.textContent),
    ).toEqual(["Leave it out", "Include patch"]);
  });

  it("is decompressed when the patch is included, and not asked again", async () => {
    const onChange = await show();
    await act(async () => button("Include patch").click());
    expect(onChange).toHaveBeenCalledWith(
      { leftOut: [], added: [], decompress: true },
      ["Game.chd", "Director's Cut.xdelta"],
    );
    expect(popUp()).toBeNull();
  });

  it("leaves the patch out, then says so until OK", async () => {
    const onChange = await show();
    await act(async () => button("Leave it out").click());
    expect(onChange).toHaveBeenCalledWith(
      { leftOut: ["Director's Cut.xdelta"], added: [] },
      ["Game.chd"],
    );
    expect(popUp()?.querySelector("h2")?.textContent).toBe("Patch left out");
    expect(popUp()?.querySelector("p")?.textContent).toBe(
      "“Director's Cut.xdelta” was left out. You can export the game as normal.",
    );
    await act(async () => button("OK").click());
    expect(popUp()).toBeNull();
  });
});
