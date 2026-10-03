import { useEffect, useState } from "react";
import * as bridge from "./bridge";
import { alsoImporting } from "./AlsoImporting";

const baseName = (path: string) => path.split(/[\\/]/).pop() || path;

/** The name of each kind of file on the list. */
function kind(role: bridge.FileRole): string {
  switch (role.kind) {
    case "game":
      return "";
    case "named":
    case "companion":
      return "required";
    case "added":
      return "added";
  }
}

/**
 * The files we copy with the game, on the details step. A line under the
 * game's file lists the others, and clicking it opens the list, where the
 * author can leave out or add a file, but nothing here must change to go on.
 */
export function GameFilesList({
  entry,
  system,
  names,
  files,
  onChange,
  onError,
}: {
  entry: string;
  system: string;
  names: string[];
  files: bridge.GameFiles;
  onChange: (files: bridge.GameFiles, names: string[]) => void;
  onError: (error: unknown) => void;
}) {
  const [open, setOpen] = useState(false);
  const [listing, setListing] = useState<bridge.Traveling | null>(null);
  useEffect(() => {
    if (!open || !entry) return;
    let current = true;
    bridge
      .travelingFiles(entry, system, files)
      .then((listed) => current && setListing(listed))
      .catch(onError);
    return () => {
      current = false;
    };
  }, [open, entry, system, files]);

  async function change(next: bridge.GameFiles) {
    try {
      const listed = await bridge.travelingFiles(entry, system, next);
      setListing(listed);
      onChange(next, bridge.travelingNames(listed));
    } catch (error) {
      onError(error);
    }
  }
  // We remove an added file from the added ones, and leave out any other.
  function leaveOut(name: string) {
    const added = files.added.filter((path) => baseName(path) !== name);
    if (added.length !== files.added.length) change({ ...files, added });
    else change({ ...files, leftOut: [...files.leftOut, name] });
  }
  function putBack(name: string) {
    change({
      ...files,
      leftOut: files.leftOut.filter((left) => left !== name),
    });
  }
  async function add() {
    const picked = await bridge.pickGameFiles();
    if (picked.length) change({ ...files, added: [...files.added, ...picked] });
  }

  const remove = (name: string) => (
    <button
      type="button"
      className="game-file-remove"
      aria-label={`Leave out ${name}`}
      onClick={() => leaveOut(name)}
    >
      ×
    </button>
  );
  return (
    <div className="traveling" data-traveling>
      <p>{names[0]}</p>
      <button
        type="button"
        className="traveling-also"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {alsoImporting(names) ?? "Files"}
      </button>
      {open && listing && (
        <div className="game-files" data-game-files>
          <ul>
            {listing.files.map((file) => {
              const fixed =
                file.role.kind === "game" ||
                file.role.kind === "named" ||
                (file.role.kind === "companion" && file.role.required);
              return (
                <li key={file.name}>
                  <span className="game-file-name">{file.name}</span>
                  <span className="game-file-kind">{kind(file.role)}</span>
                  {!fixed && remove(file.name)}
                </li>
              );
            })}
            {listing.patches.map((name) => (
              <li key={name}>
                <span className="game-file-name">{name}</span>
                <span className="game-file-kind">patch</span>
                {remove(name)}
              </li>
            ))}
            {listing.refused.map((name) => (
              <li key={name} className="game-file-refused">
                <span className="game-file-name">{name}</span>
                <span className="game-file-kind">
                  does not apply to this game
                </span>
                {remove(name)}
              </li>
            ))}
            {files.leftOut.map((name) => (
              <li key={name} className="game-file-left-out">
                <span className="game-file-name">{name}</span>
                <span className="game-file-kind">left out</span>
                <button type="button" onClick={() => putBack(name)}>
                  Put back
                </button>
              </li>
            ))}
          </ul>
          {bridge.native && (
            <button type="button" className="game-file-add" onClick={add}>
              Add file…
            </button>
          )}
        </div>
      )}
    </div>
  );
}
