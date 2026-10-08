import { describe, expect, it } from "vitest";

import { defaultHotkeys, swapPadDefaults, type Hotkeys } from "./hotkeys";

/** The default keys of the builder, with these pad bindings added. */
function withPads(pads: Partial<Record<keyof Hotkeys, string[]>>): Hotkeys {
  return Object.fromEntries(
    Object.entries(defaultHotkeys).map(([id, list]) => [
      id,
      [...list, ...(pads[id as keyof Hotkeys] ?? [])],
    ]),
  ) as Hotkeys;
}

const megaDrive = withPads({
  "quick-save": ["pad:l2"],
  "quick-load": ["pad:r2"],
});
const gameBoy = withPads({
  "quick-save": ["pad:l"],
  "quick-load": ["pad:r"],
  "previous-slot": ["pad:l2"],
  "next-slot": ["pad:r2"],
});

describe("the hotkeys of a draft whose console changes", () => {
  it("swap the pad defaults of one console for those of the other", () => {
    expect(swapPadDefaults(megaDrive, megaDrive, gameBoy)).toEqual(gameBoy);
    expect(swapPadDefaults(gameBoy, gameBoy, megaDrive)).toEqual(megaDrive);
  });

  it("keep what the author changed", () => {
    const changed = {
      ...megaDrive,
      "quick-save": ["key:f2", "pad:l2", "pad:select"],
      "quick-load": ["key:f4"],
      "previous-slot": [],
    };
    const swapped = swapPadDefaults(changed, megaDrive, gameBoy);
    expect(swapped["quick-save"]).toEqual(["key:f2", "pad:select", "pad:l"]);
    expect(swapped["quick-load"]).toEqual(["key:f4", "pad:r"]);
    expect(swapped["previous-slot"]).toEqual(["pad:l2"]);
  });

  it("add no default that another hotkey already has", () => {
    const changed = { ...megaDrive, menu: [...megaDrive.menu, "pad:l"] };
    const swapped = swapPadDefaults(changed, megaDrive, gameBoy);
    expect(swapped.menu).toContain("pad:l");
    expect(swapped["quick-save"]).toEqual(["key:f2"]);
  });
});
