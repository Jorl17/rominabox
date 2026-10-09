import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type * as bridge from "./bridge";

/** The answers we give in place of the engine, and the calls we receive. */
const engine = vi.hoisted(() => ({
  games: [] as bridge.InstalledGame[],
  backups: [] as bridge.BackupGame[],
  check: { kind: "sameGame" } as bridge.DataCheck,
  bulk: { imported: [], notHere: [], refused: [] } as bridge.BulkImport,
  calls: [] as unknown[][],
}));

vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof bridge>();
  const called =
    <T,>(name: string, answer: (...args: never[]) => T) =>
    (...args: unknown[]) => {
      engine.calls.push([name, ...args]);
      return Promise.resolve(answer(...(args as never[])));
    };
  return {
    ...actual,
    games: called("games", () => engine.games),
    readImage: (path: string) => Promise.resolve({ path, url: `blob:${path}` }),
    pickDataSave: called("pickDataSave", () => "/backups/out.zip"),
    pickDataOpen: called("pickDataOpen", () => "/backups/in.zip"),
    exportGameData: called("exportGameData", (identities: string[] | null) =>
      identities === null
        ? engine.games
        : engine.games.filter((game) => identities.includes(game.identity)),
    ),
    openGameData: called("openGameData", () => engine.backups),
    checkGameData: called("checkGameData", () => engine.check),
    importGameData: called("importGameData", () => undefined),
    importAllGameData: called("importAllGameData", () => engine.bulk),
    removeGameData: called("removeGameData", () => undefined),
  };
});

import { AppHeader } from "./AppHeader";
import { GameData } from "./GameData";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

function game(
  identity: string,
  title: string,
  more: Partial<bridge.InstalledGame> = {},
): bridge.InstalledGame {
  return {
    identity,
    title,
    system: "megadrive",
    console: "Mega Drive / Genesis",
    content: title,
    app: `/Applications/${title}.app`,
    madeWith: "0.3.0",
    playerFiles: [],
    icon: null,
    appPresent: true,
    running: false,
    fileName: `${title} data.zip`,
    data: `/data/${identity}`,
    ...more,
  };
}
const SONIC = game("aaaa", "Sonic 3", { icon: "/data/aaaa/game-icon.png" });
const KNUCKLES = game("bbbb", "Knuckles", { appPresent: false });
const GOLD = game("cccc", "Pokemon Gold", { running: true });
const backup = (from: bridge.InstalledGame, here: boolean) => ({
  ...from,
  here,
});

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  engine.games = [KNUCKLES, GOLD, SONIC];
  engine.backups = [];
  engine.check = { kind: "sameGame" };
  engine.bulk = { imported: [], notHere: [], refused: [] };
  engine.calls = [];
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

async function shown() {
  await act(async () => root.render(<GameData />));
}
function button(name: string): HTMLButtonElement {
  const found = [...document.querySelectorAll("button")].find(
    (element) => element.textContent === name,
  );
  if (!found) throw new Error(`no button ${name}`);
  return found;
}
async function click(element: HTMLElement) {
  await act(async () => element.click());
}
function rows(): string[][] {
  return [...container.querySelectorAll("tbody tr")].map((row) =>
    [...row.querySelectorAll("td")].map((cell) => cell.textContent ?? ""),
  );
}
const asked = (name: string) =>
  engine.calls.filter((call) => call[0] === name).map((call) => call.slice(1));
const popUp = () =>
  document.querySelector<HTMLElement>(".pop-up")?.textContent ?? "";

describe("the Game data section", () => {
  it("lists each game with its console and app, and offers Remove only for a game whose app is gone", async () => {
    await shown();
    expect(rows()).toEqual([
      ["", "Knuckles", "Mega Drive / Genesis", "Gone", "Remove"],
      ["", "Pokemon Gold", "Mega Drive / Genesis", "Running", ""],
      ["", "Sonic 3", "Mega Drive / Genesis", "Found", ""],
    ]);
    const icon = container.querySelector<HTMLImageElement>("tbody img");
    expect(icon?.getAttribute("src")).toBe("blob:/data/aaaa/game-icon.png");
  });

  it("says there are no games when there are none", async () => {
    engine.games = [];
    await shown();
    expect(container.textContent).toContain("No games on this computer yet.");
    expect(button("Export all").disabled).toBe(true);
  });

  it("exports the chosen games, or every game, to the zip picked", async () => {
    await shown();
    expect(button("Export chosen").disabled).toBe(true);
    await click(
      container.querySelector<HTMLInputElement>(
        '[aria-label="Choose Sonic 3"]',
      )!,
    );
    await click(button("Export chosen"));
    expect(asked("pickDataSave")).toEqual([["Sonic 3 data.zip"]]);
    expect(asked("exportGameData")).toEqual([[["aaaa"], "/backups/out.zip"]]);
    expect(container.textContent).toContain("Exported the data of 1 game.");

    await click(button("Export all"));
    expect(asked("exportGameData")[1]).toEqual([null, "/backups/out.zip"]);
    expect(container.textContent).toContain("Exported the data of 3 games.");
  });

  it("imports a backup of the same game into it after asking", async () => {
    engine.backups = [backup(SONIC, true)];
    await shown();
    await click(button("Import"));
    expect(document.querySelector("select")?.value).toBe("aaaa");
    expect(asked("checkGameData")).toEqual([["/backups/in.zip", 0, "aaaa"]]);
    expect(popUp()).toContain(
      "Importing this data will permanently replace the saves, states and settings of “Sonic 3”.",
    );
    expect(popUp()).not.toContain("This data is from");
    await click(button("Yes, import and replace my data"));
    expect(asked("importGameData")).toEqual([["/backups/in.zip", 0, "aaaa"]]);
    expect(container.textContent).toContain(
      "Imported the data into “Sonic 3”.",
    );
  });

  it("names the backup's game when the game picked is another", async () => {
    const other = game("dddd", "Sonic 3 (patched)");
    engine.backups = [backup(other, false)];
    engine.check = { kind: "otherGame", detail: other };
    await shown();
    await click(button("Import"));
    expect(button("Yes, import and replace my data").disabled).toBe(true);
    const select = document.querySelector("select")!;
    await act(async () => {
      select.value = "aaaa";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(popUp()).toContain("This data is from “Sonic 3 (patched)”.");
    await click(button("Yes, import and replace my data"));
    expect(asked("importGameData")).toEqual([["/backups/in.zip", 0, "aaaa"]]);
  });

  it("shows why a backup is refused and does not import it", async () => {
    engine.backups = [backup(SONIC, true)];
    engine.check = {
      kind: "refused",
      detail: "“Sonic 3” is open. Quit it, then try again.",
    };
    await shown();
    await click(button("Import"));
    expect(popUp()).toContain("“Sonic 3” is open. Quit it, then try again.");
    expect(button("Yes, import and replace my data").disabled).toBe(true);
    await click(button("No, keep my existing data"));
    expect(document.querySelector(".pop-up")).toBeNull();
    expect(asked("importGameData")).toEqual([]);
  });

  it("imports a bulk backup into the games here and says which are not", async () => {
    const elsewhere = game("eeee", "Streets of Rage");
    engine.backups = [backup(SONIC, true), backup(elsewhere, false)];
    engine.bulk = {
      imported: [SONIC],
      notHere: [elsewhere],
      refused: [{ game: GOLD, reason: "“Pokemon Gold” is open." }],
    };
    await shown();
    await click(button("Import"));
    expect(popUp()).toContain(
      "Importing this data will permanently replace the saves, states and settings ofSonic 3",
    );
    expect(popUp()).toContain("Not on this computerStreets of Rage");
    await click(button("Yes, import and replace my data"));
    expect(asked("importAllGameData")).toEqual([["/backups/in.zip"]]);
    const said = container.querySelector(".game-data-said")?.textContent;
    expect(said).toBe(
      "Imported the data of 1 game.\nNot on this computer: Streets of Rage.\n“Pokemon Gold”: “Pokemon Gold” is open.",
    );
  });

  it("removes a game whose app is gone only once confirmed", async () => {
    await shown();
    await click(button("Remove"));
    expect(popUp()).toContain("Remove “Knuckles”?");
    await click(button("Cancel"));
    expect(asked("removeGameData")).toEqual([]);
    await click(button("Remove"));
    const confirm = [
      ...document.querySelectorAll<HTMLButtonElement>(".pop-up button"),
    ].find((element) => element.textContent === "Remove")!;
    await click(confirm);
    expect(asked("removeGameData")).toEqual([["bbbb"]]);
    expect(container.textContent).toContain("Removed “Knuckles”.");
  });

  it("opens from the header in place of the builder, and closes again", async () => {
    await act(async () => root.render(<AppHeader />));
    expect(container.querySelector(".game-data")).toBeNull();
    await click(button("Game data"));
    expect(container.querySelector(".game-data")).not.toBeNull();
    await click(button("Builder"));
    expect(container.querySelector(".game-data")).toBeNull();
  });
});
