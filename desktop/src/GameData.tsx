import { useCallback, useEffect, useRef, useState } from "react";
import * as bridge from "./bridge";
import { ImportAll, ImportOne, quoted, RemoveGame } from "./GameDataDialogs";
import { Help } from "./Help";
import "./GameData.css";

/** The pop-up open over the list, if any. */
type Asking =
  | { kind: "one"; zip: string; backup: bridge.BackupGame }
  | { kind: "all"; zip: string; backups: bridge.BackupGame[] }
  | { kind: "remove"; game: bridge.InstalledGame };

/** What we say under the buttons after the last action. */
type Said = { text: string; failed: boolean };

const counted = (count: number) => `${count} ${count === 1 ? "game" : "games"}`;

/** What we say after importing a bulk backup. */
function bulkReport(report: bridge.BulkImport): string {
  const lines = [`Imported the data of ${counted(report.imported.length)}.`];
  if (report.notHere.length > 0)
    lines.push(
      `Not on this computer: ${report.notHere.map((game) => game.title).join(", ")}.`,
    );
  for (const { game, reason } of report.refused)
    lines.push(`${quoted(game.title)}: ${reason}`);
  return lines.join("\n");
}

/** The state of a game's app, in the App column. */
function appState(game: bridge.InstalledGame): string {
  if (game.running) return "Running";
  return game.appPresent ? "Found" : "Gone";
}

/** The games on this computer, with their saves, states and settings. Here
 * a person exports them to one zip, imports a backup and removes the data of
 * a game whose app is gone. */
export function GameData() {
  const [games, setGames] = useState<bridge.InstalledGame[] | null>(null);
  const [icons, setIcons] = useState<Record<string, string>>({});
  // The icons we asked for, so we ask for each once.
  const asked = useRef(new Set<string>());
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [asking, setAsking] = useState<Asking | null>(null);
  const [said, setSaid] = useState<Said | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    const found = await bridge.games();
    setGames(found);
    setChosen(
      (before) =>
        new Set(
          found
            .map((game) => game.identity)
            .filter((identity) => before.has(identity)),
        ),
    );
  }, []);
  useEffect(() => {
    refresh().catch((reason) =>
      setSaid({ text: String(reason), failed: true }),
    );
  }, [refresh]);
  useEffect(() => {
    for (const game of games ?? []) {
      if (!game.icon || asked.current.has(game.identity)) continue;
      asked.current.add(game.identity);
      bridge
        .readImage(game.icon)
        .then((picture) =>
          setIcons((before) => ({ ...before, [game.identity]: picture.url })),
        )
        .catch(() => {});
    }
  }, [games]);

  /** Do `work`, then say what it returns and list the games again. */
  async function act(work: () => Promise<string | null>) {
    setBusy(true);
    try {
      const text = await work();
      if (text) setSaid({ text, failed: false });
    } catch (reason) {
      setSaid({ text: String(reason), failed: true });
    } finally {
      setBusy(false);
      await refresh().catch(() => {});
    }
  }

  function exportGames(identities: string[] | null) {
    const only =
      identities?.length === 1
        ? games?.find((game) => game.identity === identities[0])
        : undefined;
    void act(async () => {
      const zip = await bridge.pickDataSave(
        only ? only.fileName : "Game data.zip",
      );
      if (!zip) return null;
      const exported = await bridge.exportGameData(identities, zip);
      return `Exported the data of ${counted(exported.length)}.`;
    });
  }

  function importBackup() {
    void act(async () => {
      const zip = await bridge.pickDataOpen();
      if (!zip) return null;
      const backups = await bridge.openGameData(zip);
      setAsking(
        backups.length === 1
          ? { kind: "one", zip, backup: backups[0] }
          : { kind: "all", zip, backups },
      );
      return null;
    });
  }

  function toggle(identity: string, on: boolean) {
    const next = new Set(chosen);
    if (on) next.add(identity);
    else next.delete(identity);
    setChosen(next);
  }

  const listed = games ?? [];
  const everyChosen = listed.length > 0 && chosen.size === listed.length;
  return (
    <section className="game-data" aria-labelledby="game-data-title">
      <div className="heading-row">
        <h1 id="game-data-title">
          Game data
          <Help label="About game data">
            The saves, states, screenshots, controls and settings of your games.
            A game is listed once it has started on this computer.
          </Help>
        </h1>
        <div className="game-data-actions">
          <button className="secondary" disabled={busy} onClick={importBackup}>
            Import
          </button>
          <button
            className="secondary"
            disabled={busy || chosen.size === 0}
            onClick={() => exportGames([...chosen])}
          >
            Export chosen
          </button>
          <button
            className="secondary"
            disabled={busy || listed.length === 0}
            onClick={() => exportGames(null)}
          >
            Export all
          </button>
        </div>
      </div>
      {said && (
        <p
          className={said.failed ? "error" : "game-data-said"}
          role={said.failed ? "alert" : "status"}
        >
          {said.text}
        </p>
      )}
      {games && listed.length === 0 && (
        <p className="game-data-empty">No games on this computer yet.</p>
      )}
      {listed.length > 0 && (
        <table className="game-data-table">
          <thead>
            <tr>
              <th>
                <input
                  type="checkbox"
                  className="checkbox-box"
                  aria-label="Choose every game"
                  checked={everyChosen}
                  onChange={(event) =>
                    setChosen(
                      new Set(
                        event.target.checked
                          ? listed.map((game) => game.identity)
                          : [],
                      ),
                    )
                  }
                />
              </th>
              <th>Game</th>
              <th>Console</th>
              <th>App</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {listed.map((game) => (
              <tr key={game.identity}>
                <td>
                  <input
                    type="checkbox"
                    className="checkbox-box"
                    aria-label={`Choose ${game.title}`}
                    checked={chosen.has(game.identity)}
                    onChange={(event) =>
                      toggle(game.identity, event.target.checked)
                    }
                  />
                </td>
                <td>
                  <span className="game-data-title">
                    {icons[game.identity] ? (
                      <img src={icons[game.identity]} alt="" />
                    ) : (
                      <span className="game-data-no-icon" />
                    )}
                    {game.title}
                  </span>
                </td>
                <td>{game.console}</td>
                <td className={game.appPresent ? "" : "game-data-gone"}>
                  {appState(game)}
                </td>
                <td>
                  {!game.appPresent && (
                    <button
                      className="text-button"
                      disabled={busy}
                      onClick={() => setAsking({ kind: "remove", game })}
                    >
                      Remove
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {asking?.kind === "one" && (
        <ImportOne
          zip={asking.zip}
          backup={asking.backup}
          games={listed}
          close={() => setAsking(null)}
          imported={(target) => {
            setAsking(null);
            void act(async () => {
              await bridge.importGameData(asking.zip, 0, target.identity);
              return `Imported the data into ${quoted(target.title)}.`;
            });
          }}
        />
      )}
      {asking?.kind === "all" && (
        <ImportAll
          backups={asking.backups}
          close={() => setAsking(null)}
          confirm={() => {
            setAsking(null);
            void act(async () =>
              bulkReport(await bridge.importAllGameData(asking.zip)),
            );
          }}
        />
      )}
      {asking?.kind === "remove" && (
        <RemoveGame
          game={asking.game}
          close={() => setAsking(null)}
          confirm={() => {
            setAsking(null);
            void act(async () => {
              await bridge.removeGameData(asking.game.identity);
              return `Removed ${quoted(asking.game.title)}.`;
            });
          }}
        />
      )}
    </section>
  );
}
