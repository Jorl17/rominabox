/**
 * What to show for a console whose core is not on disk yet.
 *
 * In the picker we show only the console's name and do not grey the option.
 * The one sentence is on the export step, as the warning before we download
 * anything.
 */

/** Consoles whose core is already on disk, or nothing if that is not known yet. */
export type Usable = ReadonlySet<string>;

/**
 * We never grey a console out in the picker.
 *
 * An empty answer means we have not read the kit. For a console missing from
 * an answer we fetch the core when we create the app.
 */
export function canExport(_usable: Usable, _id: string): boolean {
  return true;
}

/** Nothing is appended to the console's name. */
export function whyNot(_usable: Usable, _id: string): string {
  return "";
}

/**
 * Whether we will fetch the core of this console when we create the app.
 *
 * An empty answer is "not known", not "fetch everything". We fetch a core
 * only when the kit has answered and this console was not among the cores on
 * disk. `declared` is the other case in the exporter: for a console with no
 * core at all there is nothing to download, and we must not promise one.
 */
export function willDownload(
  usable: Usable,
  id: string,
  declared: boolean,
): boolean {
  return declared && usable.size > 0 && !usable.has(id);
}

/** The one sentence on the export step. Empty when we will fetch nothing. */
export function downloadNotice(name: string, fetching: boolean): string {
  return fetching ? `The ${name} core will be downloaded.` : "";
}
