/**
 * What to show for a console whose core is not on disk yet: nothing.
 *
 * In the picker we show only the console's name and do not grey the option.
 * We report what we download while the export runs, and only when we
 * download something.
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
