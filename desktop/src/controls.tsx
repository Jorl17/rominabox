import { useEffect, useLayoutEffect, useRef, useState } from "react";
import registry from "../controls.json";
import * as bridge from "./bridge";
import { ControllerScene } from "./ControllerScene";
import { ControlRow, StickRows, type RowContext } from "./ControlRows";
import { useControlCapture, type Bindings } from "./controlCapture";
import { refusalWords, type Hotkeys } from "./hotkeys";
import { mouseButtons } from "./mouse";
import { stopped, withPads } from "./padPositions";
import { PadField } from "./PadField";
import systemRegistry from "../systems.json";
import "./controls.css";

export type ControlOverride = {
  label?: string;
  key?: string;
  /** The pad position we read the control from, when it was moved. */
  pad?: string;
  /** The mouse button that also works the control: a `mouseButtons` value. */
  mouse?: string;
};
export type Controls = {
  profile?: string;
  bindings: Record<string, ControlOverride>;
};
export const emptyControls = (): Controls => ({ bindings: {} });

/**
 * Author defaults: each control's label, key, pad position and mouse button.
 * A control must not use a key or pad button of a hotkey that works while the
 * game runs. We check that against `hotkeys` with the export rules.
 */
export function ControlsEditor({
  system,
  value,
  onChange,
  hotkeys,
}: {
  system: string;
  value: Controls;
  onChange: (controls: Controls) => void;
  hotkeys: Hotkeys;
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
  const state = useRef({ value, onChange, hotkeys });
  state.current = { value, onChange, hotkeys };

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
    current.onChange({
      ...current.value,
      bindings: withPads(current.value.bindings, moved) as Controls["bindings"],
    });
  }
  /** Why we reject the controls bound as `bindings` because of the hotkeys,
   * in words for the author. Here we check only that no hotkey uses an input
   * of the game, and leave other rules to the Hotkeys section and export. */
  async function refuses(bindings: Bindings): Promise<string | null> {
    const current = state.current;
    const refusal = await bridge.checkHotkeys(current.hotkeys, system, {
      ...current.value,
      bindings: bindings as Controls["bindings"],
    });
    return refusal?.kind === "gameInput" ? refusalWords(refusal) : null;
  }
  async function choosePads(moved: Record<string, string | undefined>) {
    const refused = await refuses(
      withPads(state.current.value.bindings, moved),
    );
    if (refused) {
      setMessage(refused);
      return;
    }
    movePads(moved);
    setMessage("Pad updated.");
  }
  const binding = useControlCapture({
    controls: profile.controls,
    fixed: others,
    bindings: value.bindings,
    setKey: (id, key) => patch(id, { key }),
    movePads,
    refuses,
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
          onMove={(moved) => void choosePads(moved)}
          onMessage={setMessage}
        />
        <label>
          Mouse button
          <select
            disabled={busy}
            value={value.bindings[control.id]?.mouse ?? ""}
            onChange={(e) =>
              patch(control.id, {
                mouse: e.target.value || undefined,
              })
            }
          >
            <option value="">None</option>
            {mouseButtons.map((button) => (
              <option key={button.value} value={button.value}>
                {button.word}
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
                title={stickTitle(profile, group)}
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

/** The name of stick `group` in `profile`, from its catalog entry. */
function stickTitle(
  profile: (typeof registry.profiles)[number],
  group: string,
): string {
  const groups: Record<string, { title: string } | undefined> =
    profile.groups ?? {};
  const title = groups[group]?.title;
  // We reject a stick without one in the catalog.
  if (!title) throw new Error(`${profile.id} has no title for ${group}`);
  return title;
}
