import * as bridge from "./bridge";

/**
 * The names we list on the details step for a dropped game, the game itself
 * (a dropped patch is for a game in its folder), and the files the author
 * chose with it, as returned by the exporter. A browser file has no path, so
 * we list it by its name.
 */
export async function filesThatTravel(
  filePath: string,
  fallbackName: string,
  system: string,
  files?: bridge.GameFiles,
) {
  const alone = fallbackName ? [fallbackName] : [];
  if (!filePath) return { files: alone, entry: "", added: [] as string[] };
  const listed = await bridge.travelingFiles(filePath, system, files);
  const names = bridge.travelingNames(listed);
  if (names.length > 0)
    return {
      files: names,
      entry: listed.entry || filePath,
      added: listed.added,
    };
  return { files: alone, entry: filePath, added: listed.added };
}

const baseName = (path: string) => path.split(/[\\/]/).pop() || path;

/**
 * `files` with each of `names` left out. We remove a file the author added
 * from the added ones, and leave out any other by name, so it can come back.
 */
export function leavingOut(
  files: bridge.GameFiles,
  names: string[],
): bridge.GameFiles {
  const named = (path: string) => names.includes(baseName(path));
  const added = files.added.filter((path) => !named(path));
  const beside = names.filter(
    (name) => !files.added.some((path) => baseName(path) === name),
  );
  return { ...files, added, leftOut: [...files.leftOut, ...beside] };
}

/** What we do with the files of the game on the details step. */
export type FilesProps = {
  entry: string;
  system: string;
  files: bridge.GameFiles;
  onChange: (files: bridge.GameFiles, names: string[]) => void;
  onError: (error: unknown) => void;
};

/** The game's files with `next` chosen, as returned by the exporter. */
export async function choose(props: FilesProps, next: bridge.GameFiles) {
  try {
    const listed = await bridge.travelingFiles(props.entry, props.system, next);
    props.onChange(next, bridge.travelingNames(listed));
    return listed;
  } catch (error) {
    props.onError(error);
    return null;
  }
}
