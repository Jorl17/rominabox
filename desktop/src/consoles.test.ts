import { describe, expect, it } from "vitest";
import { canExport, whyNot } from "./consoles";

describe("whether a console can make a game", () => {
  const answered = new Set(["megadrive", "gb"]);

  it("says yes for a console the kit resolved", () => {
    expect(canExport(answered, "megadrive")).toBe(true);
    expect(whyNot(answered, "megadrive")).toBe("");
  });

  it("says no, briefly, for one it did not", () => {
    expect(canExport(answered, "dreamcast")).toBe(false);
    expect(whyNot(answered, "dreamcast")).toBe("no core yet");
  });

  it("says yes to everything while it has not been told anything", () => {
    // The browser build has no kit to resolve against. Greying out every
    // console in the picker because there is no answer yet is worse than
    // greying out none.
    const unanswered = new Set<string>();
    expect(canExport(unanswered, "dreamcast")).toBe(true);
    expect(whyNot(unanswered, "dreamcast")).toBe("");
  });
});
