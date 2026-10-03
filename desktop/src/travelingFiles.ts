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
