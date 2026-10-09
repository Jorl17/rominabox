import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { CustomShaderCards } from "./customShaders";

const checked = vi.hoisted(() => ({
  selections: [] as unknown[],
  answer: {
    warned: [] as string[],
    warnings: [] as { text: string; detail: string }[],
  },
}));

vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./bridge")>();
  return {
    ...actual,
    shaderWarnings: (selection: unknown) => {
      checked.selections.push(selection);
      return Promise.resolve(checked.answer);
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
  checked.selections = [];
  checked.answer = { warned: [], warnings: [] };
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const deep = { name: "PAL", path: "C:/filters/pal.glslp" };
const shallow = { name: "CRT", path: "C:/filters/crt.glslp" };

function Cards({ custom }: { custom: { name: string; path: string }[] }) {
  return (
    <CustomShaderCards
      selection={[["crt-royale"], custom, null]}
      onRemove={() => {}}
    />
  );
}

it("an author's filter that will not load on a platform is marked, with the reason after the cards", async () => {
  checked.answer = {
    warned: [deep.path],
    warnings: [
      { text: "The shader “PAL” won’t load on a Mac.", detail: "Mac reason." },
      {
        text: "The shader “PAL” may not load on Windows.",
        detail: "Windows reason.",
      },
    ],
  };
  const custom = [deep, shallow];
  await act(async () => root.render(<Cards custom={custom} />));

  // We check the whole selection in the shell, as in the export.
  expect(checked.selections).toEqual([
    { bundled: ["crt-royale"], custom, initial: null },
  ]);
  const cards = [...container.querySelectorAll(".shader-card")];
  expect(cards.map((card) => !!card.querySelector(".shader-warned"))).toEqual([
    true,
    false,
  ]);
  expect(
    [...container.querySelectorAll(".shader-warnings p")].map(
      (line) => line.textContent,
    ),
  ).toEqual([
    "The shader “PAL” won’t load on a Mac.",
    "The shader “PAL” may not load on Windows.",
  ]);
});

it("with no filter of the author's the shell is not asked", async () => {
  await act(async () => root.render(<Cards custom={[]} />));
  expect(checked.selections).toEqual([]);
});
