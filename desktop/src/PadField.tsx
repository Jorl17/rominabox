import { moveTo, padPositions, positionName, positionOf } from "./padPositions";

type Control = { id: string; label: string };

/**
 * The pad position we read a control from, chosen by name, to move a control
 * without a controller at hand. To set it by a press, use the Bind button.
 * We reject a position that another offered pad uses, and pass the rest to
 * `onMove` to apply.
 */
export function PadField({
  control,
  bindings,
  movable,
  fixed,
  disabled,
  onMove,
  onMessage,
}: {
  control: Control;
  bindings: Record<string, { pad?: string } | undefined>;
  movable: Control[];
  fixed: Control[];
  disabled: boolean;
  onMove: (moved: Record<string, string | undefined>) => void;
  onMessage: (message: string) => void;
}) {
  const held = new Set(fixed.map((other) => other.id));
  function choose(to: string) {
    const moved = moveTo(movable, fixed, bindings, control.id, to);
    if (!moved) {
      onMessage(`${positionName(to)} is taken.`);
      return;
    }
    onMove(moved);
  }
  return (
    <label>
      Pad
      <select
        aria-label={`${control.label} pad`}
        value={positionOf(control, bindings)}
        disabled={disabled}
        onChange={(e) => choose(e.target.value)}
      >
        {padPositions.map((position) => (
          <option
            key={position.id}
            value={position.id}
            disabled={held.has(position.id)}
          >
            {position.name}
          </option>
        ))}
      </select>
    </label>
  );
}
