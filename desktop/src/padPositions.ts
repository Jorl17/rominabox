import registry from "../controls.json";

/**
 * A position on the standard pad, as the catalog declares it. A stick's
 * direction contains its opposite, the other half of its axis.
 */
export type PadPosition = { id: string; name: string; opposite?: string };
export const padPositions: PadPosition[] = registry.padPositions;

type Control = { id: string };
type Labelled = { id: string; label: string };
type Bindings = Record<string, { pad?: string } | undefined>;

/** Where a control is read from: the author's choice, or its own position. */
export function positionOf(control: Control, bindings: Bindings): string {
  return bindings[control.id]?.pad ?? control.id;
}

export function positionName(id: string): string {
  return padPositions.find((position) => position.id === id)?.name ?? id;
}

/**
 * Move `id` to the position `to`. A control already there moves to the old
 * position of `id`, so no two controls ever share one. `fixed` are the
 * controls that never move (those of another pad in the game). Return null
 * when one of them is at `to`. Otherwise return the pad position we read each
 * changed control from, or undefined for its default position.
 */
export function moveTo(
  movable: Control[],
  fixed: Control[],
  bindings: Bindings,
  id: string,
  to: string,
): Record<string, string | undefined> | null {
  if (fixed.some((control) => control.id === to)) return null;
  const own = (control: Control, position: string) =>
    position === control.id ? undefined : position;
  const control = movable.find((candidate) => candidate.id === id);
  if (!control) return null;
  const from = positionOf(control, bindings);
  const holder = movable.find(
    (candidate) =>
      candidate.id !== id && positionOf(candidate, bindings) === to,
  );
  const moved: Record<string, string | undefined> = { [id]: own(control, to) };
  if (holder) moved[holder.id] = own(holder, from);
  return moved;
}

/** `bindings` with each control in `moved` read from its new position. */
export function withPads<T extends { pad?: string }>(
  bindings: Record<string, T | undefined>,
  moved: Record<string, string | undefined>,
): Record<string, T | undefined> {
  const next = { ...bindings };
  for (const [id, pad] of Object.entries(moved))
    next[id] = { ...next[id], pad } as T;
  return next;
}

/**
 * The controls that would have no input in the game, and why. RetroArch reads
 * the axis of a stick as a whole, so a direction left in place stops working
 * once its opposite has moved or another control has its opposite's position.
 * We reject such a game in export with the same words (pad_positions.rs).
 */
export function stopped(
  declared: Labelled[],
  bindings: Bindings,
): Record<string, string> {
  const label = (id: string) =>
    declared.find((control) => control.id === id)?.label ?? id;
  const found: Record<string, string> = {};
  for (const control of declared) {
    if (positionOf(control, bindings) !== control.id) continue;
    const opposite = padPositions.find(
      (position) => position.id === control.id,
    )?.opposite;
    if (!opposite) continue;
    const moved = declared.find(
      (other) =>
        other.id === opposite && positionOf(other, bindings) !== opposite,
    );
    const taken = declared.find(
      (other) =>
        other.id !== opposite && positionOf(other, bindings) === opposite,
    );
    const cause = moved
      ? `${label(moved.id)} is moved`
      : taken
        ? `${label(taken.id)} is on ${positionName(opposite)}`
        : null;
    if (cause)
      found[control.id] = `${control.label} stops working while ${cause}.`;
  }
  return found;
}
