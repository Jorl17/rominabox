import { act } from "react";
import { describe, expect, it } from "vitest";

import {
  button,
  click,
  container,
  enterText,
  inspectHandlers,
  progressButton,
  waitForText,
} from "./App.testing";

describe("App workflow", () => {
  it("shows that a dropped game is being identified before the result arrives", async () => {
    let finish: (value: {
      title: string;
      system: string;
      source: "header" | "filename";
      filename: string;
      size: number;
    }) => void = () => {};
    inspectHandlers.inspect = () =>
      new Promise((resolve) => {
        finish = resolve;
      });
    const input = container.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [new File([new Uint8Array(32)], "Ape Escape (Europe).chd")],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(
      container.querySelector('progress[aria-label="Game identification"]'),
    ).not.toBeNull();
    expect(container.textContent).toContain("Finding your game");
    const progress = container.querySelector('nav[aria-label="Progress"]')!;
    expect(progressButton("Menu", progress).disabled).toBe(true);

    await act(async () => {
      finish({
        title: "Ape Escape",
        system: "ps1",
        source: "filename",
        filename: "Ape Escape (Europe).chd",
        size: 32,
      });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    inspectHandlers.inspect = null;
  });

  it("navigates through available progress steps without bypassing inspection", async () => {
    const progress = container.querySelector('nav[aria-label="Progress"]')!;
    expect(progressButton("Game", progress).getAttribute("aria-current")).toBe(
      "step",
    );
    expect(progressButton("Details", progress).disabled).toBe(true);
    expect(progressButton("Menu", progress).disabled).toBe(true);
    expect(progressButton("Export", progress).disabled).toBe(true);

    const bytes = new Uint8Array(512);
    [..."SEGA"].forEach((character, index) => {
      bytes[0x100 + index] = character.charCodeAt(0);
    });
    [..."TEST QUEST"].forEach((character, index) => {
      bytes[0x150 + index] = character.charCodeAt(0);
    });
    const input = container.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    await act(async () => {
      Object.defineProperty(input, "files", {
        configurable: true,
        value: [new File([bytes.buffer], "upload.gen")],
      });
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });

    expect(progressButton("Details", progress).disabled).toBe(false);
    // Choosing the file starts identification. The menu stays unavailable
    // until it finishes. The held-lookup test covers the wait itself.
    await waitForText("Game details");
    expect(
      container.querySelector('progress[aria-label="Game identification"]'),
    ).toBeNull();
    expect(container.querySelector("h1")?.textContent).toBe("Game details");
    expect(
      progressButton("Details", progress).getAttribute("aria-current"),
    ).toBe("step");
    expect(progressButton("Menu", progress).disabled).toBe(false);
    expect(progressButton("Export", progress).disabled).toBe(false);

    const name = container.querySelector(
      '.fields input:not([type="file"])',
    ) as HTMLInputElement;
    act(() => enterText(name, "My Test Quest"));
    act(() => click(progressButton("Game", progress)));
    act(() => click(progressButton("Menu", progress)));
    expect(container.querySelector("h1")?.textContent).toBe("Choose a menu");
    act(() => click(progressButton("Details", progress)));
    expect(
      (
        container.querySelector(
          '.fields input:not([type="file"])',
        ) as HTMLInputElement
      ).value,
    ).toBe("My Test Quest");
  });

  it("starts with only the game chooser and keeps Next disabled", () => {
    expect(container.querySelector("h1")?.textContent).toBe("Choose a game");
    expect(container.querySelector('input[type="file"]')).not.toBeNull();
    expect(container.textContent).not.toContain("App name");
    expect(button("Next").disabled).toBe(true);
  });
});
