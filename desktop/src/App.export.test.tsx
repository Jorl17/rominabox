import { act } from "react";
import { describe, expect, it } from "vitest";

import { button, click, container, openMenu } from "./App.testing";

describe("App workflow", () => {
  it("presents export as an honest disabled integration step", async () => {
    await openMenu();
    act(() => click(button("Next")));

    expect(container.querySelector("h1")?.textContent).toBe("Export your game");
    expect(container.textContent).toContain(
      "Export is available in the desktop app.",
    );
    expect(container.textContent?.toLowerCase() ?? "").not.toContain("zip");
    const saveHelp = container.querySelector(
      ".destination .help-button",
    ) as HTMLButtonElement;
    act(() => click(saveHelp));
    expect(document.body.textContent?.toLowerCase() ?? "").not.toContain("zip");
    expect(button("Create app").disabled).toBe(true);
  });
});
