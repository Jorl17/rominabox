import { useEffect, useLayoutEffect, useRef, useState } from "react";
import registry from "../controls.json";
import { ControllerScene } from "./ControllerScene";
import { ControlRow, StickRows, type RowContext } from "./ControlRows";
import { useControlCapture } from "./controlCapture";
import { stopped } from "./padPositions";
import { PadField } from "./PadField";
import systemRegistry from "../systems.json";
import "./controls.css";

export type ControlOverride = {
  label?: string;
  key?: string;
  /** The pad position we read the control from, when it was moved. */
  pad?: string;
  mouse?: number;
};
export type Controls = {
  profile?: string;
  bindings: Record<string, ControlOverride>;
};
export const emptyControls = (): Controls => ({ bindings: {} });

const MOUSE_BUTTONS = ["Left", "Right", "Middle", "Button 4", "Button 5"];

/** Author defaults: each control's label, key, pad position and mouse button. */
export function ControlsEditor({
  system,
  value,
  onChange,
}: {
  system: string;
  value: Controls;
  onChange: (controls: Controls) => void;
}) {
  const systemDefinition = systemRegistry.systems.find(
    (s) =>
      s.id === system.toLowerCase() ||
      s.aliases.some((alias) => alias.toLowerCase() === system.toLowerCase()),
  );
  const canonicalSystem = systemDefinition?.id || system;
  const variants = registry.profiles.filter((p) =>
    p.systems.includes(canonicalSystem),
  );
  const profile =
    variants.find((p) => p.id === value.profile) ||
    registry.profiles.find(
      (p) => p.id === systemDefinition?.controllerProfile,
    ) ||
    registry.profiles.find((p) => p.id === "retropad")!;
  // The controls of another offered pad stay where they are, because their
  // positions are fixed in the game, so the author cannot take any of them.
  const others = variants
    .flatMap((variant) => variant.controls)
    .filter(
      (control, index, all) =>
        !profile.controls.some((own) => own.id === control.id) &&
        all.findIndex((seen) => seen.id === control.id) === index,
    );
  const [selected, setSelected] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const buttons = useRef<Record<string, HTMLButtonElement | null>>({});
  const restoreFocus = useRef<string | null>(null);
  const state = useRef({ value, onChange });
  state.current = { value, onChange };

  function patch(id: string, update: Partial<ControlOverride>) {
    const current = state.current;
    current.onChange({
      ...current.value,
      bindings: {
        ...current.value.bindings,
        [id]: { ...current.value.bindings[id], ...update },
      },
    });
  }
  function movePads(moved: Record<string, string | undefined>) {
    const current = state.current;
    const bindings = { ...current.value.bindings };
    for (const [id, pad] of Object.entries(moved))
      bindings[id] = { ...bindings[id], pad };
    current.onChange({ ...current.value, bindings });
  }
  const binding = useControlCapture({
    controls: profile.controls,
    fixed: others,
    bindings: value.bindings,
    setKey: (id, key) => patch(id, { key }),
    movePads,
    say: setMessage,
    ended: (stop) => {
      restoreFocus.current = stop;
    },
  });
  useLayoutEffect(() => {
    // Bind buttons are disabled until React commits the capture's end.
    if (binding.capture || !restoreFocus.current) return;
    const stop = restoreFocus.current;
    restoreFocus.current = null;
    buttons.current[stop]?.focus();
  });
  useEffect(() => {
    setSelected(null);
    binding.reset();
    setMessage("");
  }, [system, profile.id]);

  if (!profile) return null;
  const busy = !!binding.capture || !!binding.pending;
  const context: RowContext = {
    bindings: value.bindings,
    capture: binding.capture,
    pending: binding.pending,
    stops: stopped([...profile.controls, ...others], value.bindings),
    busy,
    bind: (stop, members) => {
      setSelected(stop);
      binding.start(stop, members);
    },
    cancel: binding.cancel,
    useForBoth: binding.useForBoth,
    label: (id, label) => patch(id, { label }),
    button: (stop, node) => {
      buttons.current[stop] = node;
    },
    devices: (control) => (
      <>
        <PadField
          control={control}
          bindings={value.bindings}
          movable={profile.controls}
          fixed={others}
          disabled={busy}
          onMove={movePads}
          onMessage={setMessage}
        />
        <label>
          Mouse button
          <select
            disabled={busy}
            value={value.bindings[control.id]?.mouse ?? ""}
            onChange={(e) =>
              patch(control.id, {
                mouse: e.target.value ? Number(e.target.value) : undefined,
              })
            }
          >
            <option value="">None</option>
            {MOUSE_BUTTONS.map((name, i) => (
              <option key={i} value={i + 1}>
                {name}
              </option>
            ))}
          </select>
        </label>
      </>
    ),
  };
  const drawn = new Set<string>();
  return (
    <div className="controls-editor">
      <div className="controls-toolbar">
        {variants.length > 1 ? (
          <label>
            Controller{" "}
            <select
              aria-label="Controller variant"
              value={profile.id}
              disabled={busy}
              onChange={(e) => {
                const next = variants.find((p) => p.id === e.target.value)!;
                const ids = new Set(next.controls.map((c) => c.id));
                onChange({
                  profile: next.id,
                  // We reset pad positions, because a control of the new pad
                  // may be where one was moved to.
                  bindings: Object.fromEntries(
                    Object.entries(value.bindings)
                      .filter(([id]) => ids.has(id))
                      .map(([id, binding]) => [
                        id,
                        { ...binding, pad: undefined },
                      ]),
                  ),
                });
              }}
            >
              {variants.map((variant) => (
                <option key={variant.id} value={variant.id}>
                  {variant.name}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <strong>{profile.name} controls</strong>
        )}
        <button
          type="button"
          className="text-button"
          disabled={busy || !Object.keys(value.bindings).length}
          onClick={() => {
            onChange({ profile: value.profile, bindings: {} });
            binding.reset();
            setMessage("Defaults restored.");
          }}
        >
          Reset to defaults
        </button>
      </div>
      <ControllerScene
        profile={profile as never}
        bindings={value.bindings}
        selected={selected}
        onSelect={setSelected}
      />
      <table className="controls-table" aria-label={`${profile.name} controls`}>
        <thead>
          <tr>
            <th scope="col">Button</th>
            <th scope="col">Action label</th>
            <th scope="col">Binding</th>
            <th scope="col">Devices</th>
          </tr>
        </thead>
        <tbody>
          {profile.controls.map((control) => {
            const group = "group" in control ? control.group : undefined;
            if (!group) {
              return (
                <ControlRow
                  key={control.id}
                  control={control}
                  context={context}
                />
              );
            }
            if (drawn.has(group)) return null;
            drawn.add(group);
            return (
              <StickRows
                key={group}
                group={group}
                members={profile.controls.filter(
                  (member) => "group" in member && member.group === group,
                )}
                context={context}
              />
            );
          })}
        </tbody>
      </table>
      <span className="control-message" role="status">
        {message}
      </span>
    </div>
  );
}
