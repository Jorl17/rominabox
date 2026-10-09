import { useEffect, useState, type ReactNode } from "react";
import * as bridge from "./bridge";

/** A pop-up with Cancel and a button labelled `action`, which we turn off
 * while `ready` is false. Pressing it calls `confirm`. */
function Confirm({
  title,
  action,
  ready = true,
  close,
  confirm,
  children,
}: {
  title: string;
  action: string;
  ready?: boolean;
  close: () => void;
  confirm: () => void;
  children: ReactNode;
}) {
  return (
    <div className="pop-up-layer">
      <div
        className="pop-up game-data-pop-up"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="game-data-pop-up-title"
        onKeyDown={(event) => {
          if (event.key === "Escape") close();
        }}
      >
        <h2 id="game-data-pop-up-title">{title}</h2>
        {children}
        <div className="pop-up-actions">
          <button className="secondary" onClick={close} autoFocus>
            Cancel
          </button>
          <button className="primary" disabled={!ready} onClick={confirm}>
            {action}
          </button>
        </div>
      </div>
    </div>
  );
}

/** A game's title in quotation marks. */
export const quoted = (title: string) => `“${title}”`;

/** The pop-up for importing a backup of one game. The person picks the game
 * it goes into, and we pick the game with the same identity first when that
 * one is here. We check the backup against the game picked, and name the
 * backup's game when it is another game. */
export function ImportOne({
  zip,
  backup,
  games,
  close,
  imported,
}: {
  zip: string;
  backup: bridge.BackupGame;
  games: bridge.InstalledGame[];
  close: () => void;
  imported: (target: bridge.InstalledGame) => void;
}) {
  const [target, setTarget] = useState(backup.here ? backup.identity : "");
  const [check, setCheck] = useState<bridge.DataCheck | null>(null);
  useEffect(() => {
    setCheck(null);
    if (!target) return;
    let current = true;
    bridge
      .checkGameData(zip, 0, target)
      .then((found) => current && setCheck(found))
      .catch(
        (reason) =>
          current && setCheck({ kind: "refused", detail: String(reason) }),
      );
    return () => {
      current = false;
    };
  }, [zip, target]);
  const chosen = games.find((game) => game.identity === target);
  return (
    <Confirm
      title="Import game data"
      action="Replace"
      ready={!!chosen && !!check && check.kind !== "refused"}
      close={close}
      confirm={() => chosen && imported(chosen)}
    >
      <label className="game-data-target">
        Into
        <select
          value={target}
          onChange={(event) => setTarget(event.target.value)}
        >
          {!target && <option value="">Choose a game</option>}
          {games.map((game) => (
            <option key={game.identity} value={game.identity}>
              {game.title}
            </option>
          ))}
        </select>
      </label>
      {check?.kind === "otherGame" && (
        <p className="game-data-warning">
          This data is from {quoted(check.detail.title)}.
        </p>
      )}
      {check?.kind === "refused" && <p className="error">{check.detail}</p>}
      {chosen && check && check.kind !== "refused" && (
        <p>
          It replaces the saves, states and settings of {quoted(chosen.title)}.
        </p>
      )}
    </Confirm>
  );
}

/** The pop-up for importing a backup of several games into each of them
 * that is here. */
export function ImportAll({
  backups,
  close,
  confirm,
}: {
  backups: bridge.BackupGame[];
  close: () => void;
  confirm: () => void;
}) {
  const here = backups.filter((game) => game.here);
  const elsewhere = backups.filter((game) => !game.here);
  return (
    <Confirm
      title="Import game data"
      action="Replace"
      ready={here.length > 0}
      close={close}
      confirm={confirm}
    >
      {here.length > 0 && (
        <section className="pop-up-section">
          <h3>Replaces the data of</h3>
          <ul className="game-data-names">
            {here.map((game) => (
              <li key={game.identity}>{game.title}</li>
            ))}
          </ul>
        </section>
      )}
      {elsewhere.length > 0 && (
        <section className="pop-up-section">
          <h3>Not on this computer</h3>
          <ul className="game-data-names">
            {elsewhere.map((game) => (
              <li key={game.identity}>{game.title}</li>
            ))}
          </ul>
        </section>
      )}
    </Confirm>
  );
}

/** The pop-up in which we ask before removing the data of a game whose app
 * is gone. */
export function RemoveGame({
  game,
  close,
  confirm,
}: {
  game: bridge.InstalledGame;
  close: () => void;
  confirm: () => void;
}) {
  return (
    <Confirm
      title={`Remove ${quoted(game.title)}?`}
      action="Remove"
      close={close}
      confirm={confirm}
    >
      <p>
        Its app is gone. This deletes its saves, states and settings from this
        computer.
      </p>
    </Confirm>
  );
}
