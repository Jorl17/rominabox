import { act } from "react";
import { describe, expect, it } from "vitest";

import {
  assessWithCli,
  button,
  chooseConsole,
  click,
  container,
  enterText,
  inspectHandlers,
  nativeBridge,
  openDetails,
  progressButton,
  travelingHandlers,
  waitForText,
} from "./App.testing";

describe("App workflow", () => {
  it("omits firmware authoring for a console with no firmware requirement", async () => {
    await openDetails();
    expect(container.textContent).not.toContain("Add BIOS files");
  });

  it("stops at the BIOS step while a required file is missing", async () => {
    await openDetails("disc.cue");
    await chooseConsole("pcecd");
    expect(button("Next").disabled).toBe(true);
    const assessment = await assessWithCli("pcecd", []);
    const required = assessment.notices.find(
      (notice) => notice.kind === "required",
    );
    if (!required) throw new Error("the engine did not require a BIOS");
    await waitForText(required.text);
    expect(button("Next").disabled).toBe(true);
    expect(container.querySelector(".firmware-required")).not.toBeNull();
    const progress = container.querySelector('nav[aria-label="Progress"]')!;
    expect(progressButton("Menu", progress).disabled).toBe(true);
    expect(progressButton("Export", progress).disabled).toBe(true);
  });

  it("never says Ready to go beside a Next it has disabled", async () => {
    await openDetails("disc.cue");
    await chooseConsole("pcecd");
    const assessment = await assessWithCli("pcecd", []);
    const required = assessment.notices.find(
      (notice) => notice.kind === "required",
    );
    if (!required) throw new Error("the engine did not require a BIOS");
    await waitForText(required.text);
    expect(button("Next").disabled).toBe(true);
    expect(container.textContent).not.toContain("Ready to go");
    expect(container.textContent).toContain("needs a BIOS file");

    // And the other way, so this is not simply "never say it": a console whose
    // BIOS is optional is ready, and we must not say that it requires one.
    await chooseConsole("ps1");
    const optional = assessment.notices.length
      ? await assessWithCli("ps1", [])
      : null;
    if (!optional?.notices.some((notice) => notice.kind === "optional")) {
      throw new Error("the engine did not explain the optional BIOS");
    }
    await waitForText("Ready to go");
    expect(button("Next").disabled).toBe(false);
    expect(container.textContent).not.toContain("needs a BIOS file");
  });

  it("does not stop for a console whose BIOS is optional", async () => {
    await openDetails("disc.cue");
    await chooseConsole("ps1");
    const assessment = await assessWithCli("ps1", []);
    const optional = assessment.notices.find(
      (notice) => notice.kind === "optional",
    );
    if (!optional)
      throw new Error("the engine did not explain the optional BIOS");
    // The button is labelled Optional, and its help describes the BIOS.
    await waitForText("Add BIOS files (optional)");
    expect(container.textContent).not.toContain(optional.text);
    const help = container.querySelector(".firmware-picker .help-button");
    act(() => (help as HTMLButtonElement).focus());
    expect(document.body.textContent).toContain(optional.text);
    expect(button("Next").disabled).toBe(false);
    expect(container.querySelector(".firmware-required")).toBeNull();
    const progress = container.querySelector('nav[aria-label="Progress"]')!;
    expect(progressButton("Menu", progress).disabled).toBe(false);
    expect(progressButton("Export", progress).disabled).toBe(false);
  });

  it("explains a BIOS file that does not match this console", async () => {
    await openDetails("disc.cue");
    await chooseConsole("pcecd");
    const input = container.querySelector(
      "input[data-firmware]",
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [new File([""], "notes.txt")],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
    const assessment = await assessWithCli("pcecd", ["notes.txt"]);
    const reason = assessment.files[0]?.reason;
    if (!reason)
      throw new Error("the engine did not explain the unmatched file");
    await waitForText(reason);
    expect(button("Next").disabled).toBe(true);
  });

  it("continues once the required BIOS file is provided", async () => {
    await openDetails("disc.cue");
    await chooseConsole("pcecd");
    const input = container.querySelector(
      "input[data-firmware]",
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [new File([""], "syscard3.pce")],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
    const assessment = await assessWithCli("pcecd", ["syscard3.pce"]);
    const ready = assessment.notices.find((notice) => notice.kind === "ready");
    if (!ready) throw new Error("the engine did not accept the BIOS file");
    await waitForText(ready.text);
    expect(button("Next").disabled).toBe(false);
    expect(container.querySelector(".firmware-required")).toBeNull();
  });

  it("inspects a local ROM, allows edits, and preserves them after Back", async () => {
    await openDetails();
    expect(container.textContent).toContain("Details from your file");
    expect(container.textContent).toContain("upload.gen");

    const name = container.querySelector(
      '.fields input:not([type="file"])',
    ) as HTMLInputElement;
    expect(name.value).toBe("TEST QUEST");
    act(() => enterText(name, "My Test Quest"));
    act(() => click(button("Next")));
    act(() => click(button("Back")));

    expect(
      (
        container.querySelector(
          '.fields input:not([type="file"])',
        ) as HTMLInputElement
      ).value,
    ).toBe("My Test Quest");
  });

  /** A dropped game that becomes another game with the Director's Cut patch
   * beside it. We look up a name for each, with and without the patch. */
  async function dropPatchedGame() {
    const patch = "Director's Cut.xdelta";
    travelingHandlers.list = async (path, _system, files) => ({
      entry: path,
      files: ["Sonic 3D Blast.md"],
      patches: files?.leftOut.includes(patch) ? [] : [patch],
    });
    inspectHandlers.inspect = async () => ({
      title: "Sonic 3D Blast - Director's Cut",
      system: "megadrive",
      source: "filename",
      filename: "Sonic 3D Blast.md",
      size: 32,
    });
    await dropNamed("Sonic 3D Blast.md");
    await waitForText("Also importing");
    // We look it up again in the app, through the engine.
    nativeBridge.on = true;
    nativeBridge.inspectGame = async (_path, _online, _system, files) => ({
      title: files?.leftOut.includes(patch)
        ? "Sonic 3D Blast"
        : "Sonic 3D Blast - Director's Cut",
      system: "megadrive",
      filename: "Sonic 3D Blast.md",
      size: 32,
      source: "catalog",
      matched: true,
      warnings: [],
    });
    return patch;
  }
  async function leaveOut(name: string) {
    const selector = `[aria-label="Leave out ${name}"]`;
    for (
      let attempt = 0;
      attempt < 50 && !container.querySelector(selector);
      attempt += 1
    )
      await act(async () => new Promise((resolve) => setTimeout(resolve, 20)));
    act(() => click(container.querySelector(selector) as HTMLElement));
  }
  const gameName = () =>
    container.querySelector(
      '.fields input:not([type="file"])',
    ) as HTMLInputElement;

  it("names the game again when its patch is left out", async () => {
    const patch = await dropPatchedGame();
    expect(gameName().value).toBe("Sonic 3D Blast - Director's Cut");
    act(() => click(container.querySelector(".traveling-also")!));
    await leaveOut(patch);
    await waitForText("1 file left out");
    for (
      let attempt = 0;
      attempt < 50 && gameName().value !== "Sonic 3D Blast";
      attempt += 1
    )
      await act(async () => new Promise((resolve) => setTimeout(resolve, 20)));
    expect(gameName().value).toBe("Sonic 3D Blast");
  });

  it("keeps a name the author typed when the patch is left out", async () => {
    const patch = await dropPatchedGame();
    act(() => enterText(gameName(), "My Sonic"));
    act(() => click(container.querySelector(".traveling-also")!));
    await leaveOut(patch);
    await waitForText("1 file left out");
    await act(async () => new Promise((resolve) => setTimeout(resolve, 200)));
    expect(gameName().value).toBe("My Sonic");
  });

  async function dropWithCompanions(name: string, files: string[]) {
    const path = `/games/${name}`;
    travelingHandlers.list = async () => ({ entry: path, files });
    inspectHandlers.inspect = async () => ({
      title: "Sonic Adventure 2",
      system: "dreamcast",
      source: "filename",
      filename: name,
      size: 32,
    });
    const file = new File([new Uint8Array(32)], name) as File & {
      path?: string;
    };
    file.path = path;
    const input = container.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [file],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
    for (let attempt = 0; attempt < 50; attempt += 1) {
      const also = container.querySelector(".traveling-also");
      if (
        container.querySelector("h1")?.textContent === "Game details" &&
        also
      ) {
        return also;
      }
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 20));
      });
    }
    throw new Error(
      "the Also importing line did not name the files that travel",
    );
  }

  it("names a disc's tracks on one short line, without repeating the game", async () => {
    const stem = "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es)";
    const also = await dropWithCompanions(`${stem}.gdi`, [
      `${stem}.gdi`,
      `${stem} (Track 1).bin`,
      `${stem} (Track 2).bin`,
      `${stem} (Track 3).bin`,
    ]);
    expect(also.textContent).toBe(
      "Also importing: (Track 1).bin, (Track 2).bin, (Track 3).bin",
    );
    expect(also.textContent).not.toContain(stem);
  });

  it("counts companions once there are more than a handful", async () => {
    const also = await dropWithCompanions("Long Disc.cue", [
      "Long Disc.cue",
      ...Array.from({ length: 6 }, (_, index) => `track${index + 1}.bin`),
    ]);
    expect(also.textContent).toBe("Also importing 6 files");
  });

  async function dropNamed(name: string) {
    const path = `/games/${name}`;
    const file = new File([new Uint8Array(32)], name) as File & {
      path?: string;
    };
    file.path = path;
    const input = container.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [file],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await waitForText("Game details");
    return path;
  }

  it("puts the console back when inspection rejects it, and keeps the message", async () => {
    const stem = "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es)";
    travelingHandlers.list = async () => ({
      entry: `/games/${stem}.gdi`,
      files: [
        `${stem}.gdi`,
        `${stem} (Track 1).bin`,
        `${stem} (Track 2).bin`,
        `${stem} (Track 3).bin`,
      ],
    });
    inspectHandlers.inspect = async () => ({
      title: "Sonic Adventure 2",
      system: "dreamcast",
      source: "filename",
      filename: `${stem}.gdi`,
      size: 32,
    });
    await dropNamed(`${stem}.gdi`);
    const also = container.querySelector(".traveling-also")?.textContent;
    nativeBridge.on = true;
    nativeBridge.inspectGame = async (_path, _online, system) => {
      if (system === "ps1")
        throw new Error("PlayStation does not support .gdi files.");
      return {
        title: "Sonic Adventure 2",
        system: "dreamcast",
        filename: `${stem}.gdi`,
        size: 32,
        source: "filename",
        matched: false,
        warnings: [],
      };
    };
    await chooseConsole("ps1");
    await waitForText("PlayStation does not support .gdi files.");
    const select = container.querySelector(
      ".fields select",
    ) as HTMLSelectElement;
    expect(
      select.value,
      "a console inspection rejects has to stay the one that was valid, or export copies a different set than the line",
    ).toBe("dreamcast");
    expect(container.querySelector(".error")?.textContent).toBe(
      "PlayStation does not support .gdi files.",
    );
    expect(container.querySelector(".traveling-also")?.textContent).toBe(also);
  });

  it("puts the console back in the browser walk when inspection rejects it", async () => {
    const stem = "Sonic Adventure 2 (Europe) (En,Ja,Fr,De,Es)";
    travelingHandlers.list = async () => ({
      entry: `/games/${stem}.gdi`,
      files: [`${stem}.gdi`, `${stem} (Track 1).bin`],
    });
    inspectHandlers.inspect = async () => ({
      title: "Sonic Adventure 2",
      system: "dreamcast",
      source: "filename",
      filename: `${stem}.gdi`,
      size: 32,
    });
    await dropNamed(`${stem}.gdi`);
    await chooseConsole("ps1");
    await waitForText("PlayStation does not support .gdi files.");
    const select = container.querySelector(
      ".fields select",
    ) as HTMLSelectElement;
    expect(select.value).toBe("dreamcast");
    expect(container.querySelector(".traveling-also")?.textContent).toBe(
      "Also importing: (Track 1).bin",
    );
  });

  it("shows the exporter's refusal instead of a one-file line", async () => {
    const refusal = "missing sub file game.sub (from /games/game.ccd)";
    travelingHandlers.list = async () => {
      throw new Error(refusal);
    };
    inspectHandlers.inspect = async () => ({
      title: "Game",
      system: "pcecd",
      source: "filename",
      filename: "game.ccd",
      size: 32,
    });
    const file = new File([new Uint8Array(32)], "game.ccd") as File & {
      path?: string;
    };
    file.path = "/games/game.ccd";
    const input = container.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [file],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
      await new Promise((resolve) => setTimeout(resolve, 30));
    });
    await waitForText(refusal);
    expect(container.querySelector(".error")?.textContent).toBe(refusal);
    expect(
      container.querySelector("[data-traveling]"),
      "a refused disc is not a one-file game",
    ).toBeNull();
  });
});
