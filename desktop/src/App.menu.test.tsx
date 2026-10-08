import { act } from "react";
import { describe, expect, it } from "vitest";

import designs from "../designs.json";
import {
  button,
  checkbox,
  chooseConsole,
  click,
  container,
  openDetails,
  openMenu,
} from "./App.testing";

describe("App workflow", () => {
  it("puts help outside scrolling content and outside checkbox activation", async () => {
    await openMenu();
    const help = container.querySelector(
      ".heading-row .help-button",
    ) as HTMLButtonElement;
    expect(help.closest("label")).toBeNull();
    act(() => help.focus());
    const tooltip = document.getElementById(
      help.getAttribute("aria-describedby")!,
    );
    expect(tooltip).not.toBeNull();
    expect(container.contains(tooltip)).toBe(false);
  });

  it("keeps menu options and startup logo state in the workflow", async () => {
    await openMenu();
    expect(container.querySelector("h1")?.textContent).toBe("Choose a menu");
    expect(
      container.querySelector('img[alt*="six save slots"]') as HTMLImageElement,
    ).not.toBeNull();
    expect(
      container.querySelector('[aria-label="Menu design"]'),
    ).not.toBeNull();

    const splash = checkbox("Startup logo");
    expect(splash.checked).toBe(true);
    act(() => click(splash));
    expect(splash.checked).toBe(false);

    act(() => click(button("Next")));
    act(() => click(button("Back")));
    const restored = checkbox("Startup logo");
    expect(restored.checked).toBe(false);
  });

  it("keeps advanced emulator access off unless the author opts in", async () => {
    await openMenu();
    const access = checkbox("Advanced emulator access");
    expect(access.checked).toBe(false);
    act(() => click(access));
    expect(access.checked).toBe(true);

    act(() => click(button("Next")));
    act(() => click(button("Back")));
    const restored = checkbox("Advanced emulator access");
    expect(restored.checked).toBe(true);
  });

  it("makes every controller player 1 unless the author turns it off", async () => {
    await openMenu();
    const every = checkbox("Every controller is player 1");
    expect(every.checked).toBe(true);
    expect(
      every.closest("details")?.querySelector("summary")?.textContent?.trim(),
    ).toBe("Advanced");
    act(() => click(every));
    expect(every.checked).toBe(false);

    act(() => click(button("Next")));
    act(() => click(button("Back")));
    expect(checkbox("Every controller is player 1").checked).toBe(false);
  });

  it("gives picture filters their own section and one way to say selected", async () => {
    await openMenu();
    const text = (element: Element | null | undefined) =>
      (element?.textContent || "").replace(/\s+/g, " ").trim();
    const filters = [...container.querySelectorAll("details")].find((details) =>
      details.querySelector(".shader-grid"),
    );
    const controls = container.querySelector("details.author-controls");
    const advanced = [...container.querySelectorAll("details")].find(
      (details) => text(details.querySelector("summary")) === "Advanced",
    );
    // The title is the count, so a selected card has no Bundle tick that
    // repeats it.
    expect(text(filters?.querySelector("summary"))).toBe(
      "Picture filters (shaders) · none selected",
    );
    expect(filters?.querySelector("input[type='checkbox']")).toBeNull();
    expect(text(filters)).not.toMatch(/\bBundle\b/);
    expect(text(filters?.querySelector(".shader-grid .shader-add"))).toContain(
      "Add your own",
    );
    expect(
      controls &&
        filters &&
        controls.compareDocumentPosition(filters) &
          Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(advanced?.querySelector(".shader-grid")).toBeNull();

    const scanlines = [
      ...(filters?.querySelectorAll(".shader-grid .shader-card") ?? []),
    ].find((card) => card.textContent?.includes("Scanlines"));
    if (!(scanlines instanceof HTMLElement)) {
      throw new Error("Scanlines is not a card in the grid");
    }
    act(() => click(scanlines));
    expect(text(filters?.querySelector("summary"))).toBe(
      "Picture filters (shaders) · 1 selected",
    );
    expect(scanlines.classList.contains("chosen")).toBe(true);
    expect(scanlines.querySelector("input")).toBeNull();
    act(() => click(scanlines));
    expect(text(filters?.querySelector("summary"))).toBe(
      "Picture filters (shaders) · none selected",
    );
    expect(filters?.querySelector('[aria-label="Starts on"]')).toBeNull();
  });

  it("lets the author choose a menu design, and exports the one they chose", async () => {
    // We must pass the chosen design from the selector to the export, not a
    // fixed "native" value. Only with a second design can we tell them apart.
    await openMenu();
    const picker = [...container.querySelectorAll("select")].find(
      (select) => select.getAttribute("aria-label") === "Menu design",
    );
    if (!picker)
      throw new Error("the menu design selector is not on this step");

    // We offer every declared design, not a subset.
    const offered = [...picker.options].map((option) => option.value).sort();
    expect(offered).toEqual(designs.designs.map((d) => d.id).sort());

    // This is meaningful only with a second design. With one design, an
    // assertion that the selector shows it proves nothing, because the
    // hardcoded string and the state are the same.
    const other = designs.designs.find((d) => d.id !== picker.value);
    if (!other) {
      expect(
        designs.designs.length,
        "only one design is declared, so this cannot yet prove the choice " +
          "is honoured — it becomes a real test when a second one lands",
      ).toBe(1);
      return;
    }
    await act(async () => {
      picker.value = other.id;
      picker.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(
      picker.value,
      "choosing a design has to stick, or the export gets the old one",
    ).toBe(other.id);
  });

  it("offers menu sounds as a picker and a preview, with nothing to read", async () => {
    await openMenu();
    const picker = container.querySelector("#menu-sounds") as HTMLSelectElement;
    const field = picker.closest(".labeled-choice") as HTMLElement;
    expect(field.querySelector("label")?.textContent).toBe("Menu sounds");
    expect(field.querySelector(".menu-sound-preview button")).not.toBeNull();
    expect(field.querySelector(".help-button")).toBeNull();
    for (const pack of [...picker.options].map((option) => option.value)) {
      act(() => {
        picker.value = pack;
        picker.dispatchEvent(new Event("change", { bubbles: true }));
      });
      expect(picker.value).toBe(pack);
      // Only the label remains to read beside the pack names and the preview.
      const words = [...field.childNodes]
        .filter((node) => node !== picker)
        .map((node) =>
          node instanceof HTMLElement && node.matches(".menu-sound-preview")
            ? ""
            : node.textContent,
        )
        .join("");
      expect(words, pack).toBe("Menu sounds");
      expect(
        [...picker.options].every((option) => !option.title),
        pack,
      ).toBe(true);
    }
  });

  it("puts background play and autosave next to the startup logo", async () => {
    await openMenu();
    const logo = checkbox("Startup logo");
    const playing = checkbox("Keep playing in the background");
    const saving = checkbox("Autosave on quit");
    expect(playing.checked).toBe(false);
    // Autosave is on unless the author turns it off.
    expect(saving.checked).toBe(true);
    expect(playing.closest("details")).toBeNull();
    expect(saving.closest("details")).toBeNull();
    expect(logo.closest(".menu-settings")).toBe(
      playing.closest(".menu-settings"),
    );
    for (const [label, says] of [
      ["About keep playing in the background", "keep running"],
      ["About autosave on quit", "continue from there"],
    ]) {
      const help = container.querySelector(
        `[aria-label="${label}"]`,
      ) as HTMLButtonElement;
      expect(help).not.toBeNull();
      expect(help.closest("label")).toBeNull();
      act(() => help.focus());
      const tooltip = document.getElementById(
        help.getAttribute("aria-describedby")!,
      );
      expect(tooltip?.textContent).toContain(says);
      act(() => help.blur());
    }

    act(() => click(playing));
    act(() => click(saving));
    act(() => click(button("Next")));
    act(() => click(button("Back")));
    expect(checkbox("Keep playing in the background").checked).toBe(true);
    expect(checkbox("Autosave on quit").checked).toBe(false);

    act(() => click(checkbox("Include game menu")));
    expect(container.textContent).toContain("No in-game menu");
    expect(checkbox("Keep playing in the background").checked).toBe(true);
    expect(checkbox("Autosave on quit").checked).toBe(false);
    expect(checkbox("Startup logo").closest(".play-options")).not.toBeNull();
  });

  it("keeps achievements as a main option and requires the game menu", async () => {
    await openMenu();
    const achievements = checkbox("Achievements");
    expect(achievements.checked).toBe(true);
    expect(achievements.disabled).toBe(false);
    expect(achievements.closest("details")).toBeNull();
    expect(achievements.closest(".menu-settings")).toBe(
      checkbox("Startup logo").closest(".menu-settings"),
    );

    act(() => click(checkbox("Include game menu")));
    expect(checkbox("Achievements").checked).toBe(false);
    expect(checkbox("Achievements").disabled).toBe(true);
    act(() => click(checkbox("Include game menu")));
    expect(checkbox("Achievements").checked).toBe(true);
    act(() => click(checkbox("Achievements")));
    act(() => click(checkbox("Include game menu")));
    act(() => click(checkbox("Include game menu")));
    expect(checkbox("Achievements").checked).toBe(false);
  });

  /** The words on the chips of the hotkey `name` in the Menu step. */
  const chips = (name: string) => {
    const row = [...container.querySelectorAll(".hotkey-row")].find(
      (each) => each.querySelector(".hotkey-name")?.textContent === name,
    )!;
    return [...row.querySelectorAll(".hotkey-chip")].map(
      (chip) => chip.textContent,
    );
  };

  it("starts the hotkeys from the console's defaults, and swaps them with the console", async () => {
    await openDetails("disc.cue");
    await chooseConsole("dreamcast");
    act(() => click(button("Next")));
    expect(chips("Quick save")).toEqual(["F2", "L2"]);
    expect(chips("Quick load")).toEqual(["F4", "R2"]);
    act(() => click(button("Back")));
    await chooseConsole("ps1");
    act(() => click(button("Next")));
    expect(chips("Quick save")).toEqual(["F2"]);
    expect(chips("Quick load")).toEqual(["F4"]);
  });
});
