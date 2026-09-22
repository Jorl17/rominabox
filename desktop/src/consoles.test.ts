import { describe, expect, it } from "vitest";
import { canExport, downloadNotice, whyNot, willDownload } from "./consoles";

describe("whether a console can make a game", () => {
  const answered = new Set(["megadrive", "gb"]);

  it("says yes for a console the kit resolved", () => {
    expect(canExport(answered, "megadrive")).toBe(true);
    expect(whyNot(answered, "megadrive")).toBe("");
  });

  it("still offers a console whose core is not on disk", () => {
    // The option is only the console's name, with no note about a missing
    // core. We report the download later, on the export step.
    expect(whyNot(answered, "dreamcast")).toBe("");
    expect(canExport(answered, "dreamcast")).toBe(true);
    expect(willDownload(answered, "dreamcast", true)).toBe(true);
    expect(downloadNotice("Dreamcast", true)).toBe(
      "The Dreamcast core will be downloaded.",
    );
    expect(downloadNotice("Dreamcast", false)).toBe("");
  });

  it("does not promise a download for a console that declares no core", () => {
    expect(willDownload(answered, "dreamcast", false)).toBe(false);
    expect(downloadNotice("Dreamcast", false)).toBe("");
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
