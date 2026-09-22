/**
 * Whether we can make a game for a console, and what to show when we cannot.
 *
 * In the builder we resolve every declared console against the prepared kit
 * and get back the ones that have a core. We report a console without one
 * before the person chooses a ROM, a title, an icon and a menu, not as a
 * refusal on the export step.
 *
 * We use this module everywhere, so the check and its wording exist once.
 */

/** The answer from the kit, or nothing before there is one. */
export type Usable = ReadonlySet<string>;

/**
 * Can we make a game for this console?
 *
 * An empty answer means "not known", not "none". The browser build has no kit
 * to resolve against, and greying out every console in the picker because
 * there is no answer yet is worse than greying out none.
 */
export function canExport(usable: Usable, id: string): boolean {
  return usable.size === 0 || usable.has(id);
}

/** Three words for a console that has no core. Empty when it has one. */
export function whyNot(usable: Usable, id: string): string {
  return canExport(usable, id) ? "" : "no core yet";
}
