import { useEffect, useState } from "react";
import { ChevronRight } from "lucide-react";
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

/** What we do with the files of the game on the details step. */
type FilesProps = {
  entry: string;
  system: string;
  files: bridge.GameFiles;
  onChange: (files: bridge.GameFiles, names: string[]) => void;
  onError: (error: unknown) => void;
};

/** The game's files with `next` chosen, as returned by the exporter. */
async function choose(props: FilesProps, next: bridge.GameFiles) {
  try {
    const listed = await bridge.travelingFiles(props.entry, props.system, next);
    props.onChange(next, bridge.travelingNames(listed));
    return listed;
  } catch (error) {
    props.onError(error);
    return null;
  }
}

/** How to add files to the game in More details: the file picker of the app. */
export function AddGameFiles(props: FilesProps) {
  if (!bridge.native || !props.entry) return null;
  async function add() {
    const picked = await bridge.pickGameFiles();
    if (picked.length)
      await choose(props, {
        ...props.files,
        added: [...props.files.added, ...picked],
      });
  }
  return (
    <button type="button" className="secondary game-file-add" onClick={add}>
      Add files…
    </button>
  );
}

/**
 * The game's file, on the details step. When we copy other files with it, a
 * line under it lists them, and clicking it opens a list where the author can
 * leave a file out and put it back. For a one-file game we show only the name.
 */
export function GameFilesList(props: FilesProps & { names: string[] }) {
  const { entry, system, names, files, onError } = props;
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
    const listed = await choose(props, next);
    if (listed) setListing(listed);
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
  const leftOut = files.leftOut.length;
  const line =
    alsoImporting(names) ??
    (leftOut
      ? `${leftOut} ${leftOut === 1 ? "file" : "files"} left out`
      : null);

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
      {line && (
        <button
          type="button"
          className="traveling-also"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
        >
          <ChevronRight size={12} aria-hidden />
          <span>{line}</span>
        </button>
      )}
      {open && line && listing && (
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
        </div>
      )}
    </div>
  );
}
