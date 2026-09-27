import registry from "../controls.json";

/** A position on the standard pad, as the catalog declares it. */
export type PadPosition = { id: string; name: string };
export const padPositions: PadPosition[] = registry.padPositions;

type Control = { id: string };
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
 * controls that never move (those of a stick, and of another pad in the game).
 * Return null when one of them is at `to`. Otherwise return the pad position
 * we read each changed control from, or undefined for its default position.
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
