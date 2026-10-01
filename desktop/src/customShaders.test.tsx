import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { CustomShaderCard, useShaderWarnings } from "./customShaders";

const checked = vi.hoisted(() => ({
  selections: [] as unknown[],
  answer: [] as { path: string; sentence: string }[],
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
  checked.answer = [];
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const deep = { name: "PAL", path: "C:/filters/pal.glslp" };
const shallow = { name: "CRT", path: "C:/filters/crt.glslp" };

function Cards({ custom }: { custom: { name: string; path: string }[] }) {
  const warnings = useShaderWarnings(["crt-royale"], custom, null);
  return (
    <>
      {custom.map((shader) => (
        <CustomShaderCard
          key={shader.path}
          shader={shader}
          warning={warnings[shader.path]}
          onRemove={() => {}}
        />
      ))}
    </>
  );
}

it("an author's filter a Windows game may not load says so on its card", async () => {
  checked.answer = [{ path: deep.path, sentence: "On Windows this filter may not load." }];
  const custom = [deep, shallow];
  await act(async () => root.render(<Cards custom={custom} />));

  // We check the whole selection in the shell, as in the export.
  expect(checked.selections).toEqual([{ bundled: ["crt-royale"], custom, initial: null }]);
  const cards = [...container.querySelectorAll(".shader-card")];
  expect(cards.map((card) => card.querySelector(".shader-warning")?.textContent ?? null)).toEqual([
    "On Windows this filter may not load.",
    null,
  ]);
});

it("with no filter of the author's the shell is not asked", async () => {
  await act(async () => root.render(<Cards custom={[]} />));
  expect(checked.selections).toEqual([]);
});
