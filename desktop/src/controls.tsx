import { useEffect, useLayoutEffect, useRef, useState } from "react";
import registry from "../controls.json";
import systemRegistry from "../systems.json";
import "./controls.css";

export type ControlOverride = {
  label?: string;
  key?: string;
  button?: string;
  axis?: string;
  mouse?: number;
};
export type Controls = {
  profile?: string;
  bindings: Record<string, ControlOverride>;
};
export const emptyControls = (): Controls => ({ bindings: {} });

const keyNames: Record<string, string> = {
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
  enter: "Enter",
  space: "Space",
  rshift: "Right Shift",
  lshift: "Left Shift",
  backspace: "Backspace",
  tab: "Tab",
  del: "Delete",
  escape: "Esc",
  lctrl: "Left Ctrl",
  rctrl: "Right Ctrl",
  lalt: "Left Alt",
  ralt: "Right Alt",
};
function keyName(key: string) {
  return keyNames[key] || key.toUpperCase();
}
function capturedKey(event: KeyboardEvent): string | null {
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  return (
    (
      {
        ArrowUp: "up",
        ArrowDown: "down",
        ArrowLeft: "left",
        ArrowRight: "right",
        Enter: "enter",
        Space: "space",
        ShiftLeft: "lshift",
        ShiftRight: "rshift",
        ControlLeft: "lctrl",
        ControlRight: "rctrl",
        AltLeft: "lalt",
        AltRight: "ralt",
        Backspace: "backspace",
        Tab: "tab",
        Delete: "del",
      } as Record<string, string>
    )[event.code] || null
  );
}

/** Author defaults. Physical controller bindings remain device-specific player settings. */
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
  const [selected, setSelected] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [seconds, setSeconds] = useState(10);
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState<{
    key: string;
    conflicts: string[];
  } | null>(null);
  const captureButtons = useRef<Record<string, HTMLButtonElement | null>>({});
  const restoreFocus = useRef(false);
  const state = useRef({ value, onChange, selected });
  state.current = { value, onChange, selected };

  useEffect(() => {
    setSelected(null);
    setCapturing(false);
    setPending(null);
    setMessage("");
  }, [system, profile.id]);
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
  function focusSelected() {
    restoreFocus.current = true;
  }
  useLayoutEffect(() => {
    // Capture buttons are disabled until React commits the state change.
    if (capturing || !restoreFocus.current) return;
    restoreFocus.current = false;
    const id = state.current.selected;
    if (id) captureButtons.current[id]?.focus();
  });
  function cancel() {
    setCapturing(false);
    setPending(null);
    setMessage("Binding unchanged.");
    focusSelected();
  }
  function startCapture(id: string) {
    setSelected(id);
    setPending(null);
    setMessage("");
    setSeconds(10);
    setCapturing(true);
  }
  useEffect(() => {
    if (!capturing || !profile) return;
    const deadline = performance.now() + 10000;
    // Capture begins after the initiating click/keypress. The opening event is never a candidate.
    const timer = window.setInterval(() => {
      const remaining = Math.max(
        0,
        Math.ceil((deadline - performance.now()) / 1000),
      );
      setSeconds(remaining);
      if (!remaining) cancel();
    }, 100);
    const listen = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.repeat) return;
      if (event.key === "Escape") {
        cancel();
        return;
      }
      const key = capturedKey(event);
      if (!key) {
        setMessage("Choose a letter, number, arrow or modifier key.");
        return;
      }
      // Keep the player's menu, fullscreen and quit shortcuts out of game defaults.
      if (["q", "f"].includes(key)) {
        setMessage("That key is reserved for player shortcuts.");
        return;
      }
      const id = state.current.selected;
      if (!id) return;
      const conflicts = profile.controls
        .filter(
          (item) =>
            item.id !== id &&
            (state.current.value.bindings[item.id]?.key ?? item.key) === key,
        )
        .map((item) => item.label);
      setCapturing(false);
      if (conflicts.length) setPending({ key, conflicts });
      else {
        patch(id, { key });
        setMessage("Binding updated.");
        focusSelected();
      }
    };
    const blur = () => cancel();
    window.addEventListener("keydown", listen, true);
    window.addEventListener("blur", blur);
    return () => {
      clearInterval(timer);
      window.removeEventListener("keydown", listen, true);
      window.removeEventListener("blur", blur);
    };
  }, [capturing, profile]);

  if (!profile) return null;
  return (
    <div className="controls-editor">
      <div className="controls-toolbar">
        {variants.length > 1 ? (
          <label>
            Controller{" "}
            <select
              aria-label="Controller variant"
              value={profile.id}
              disabled={capturing}
              onChange={(e) => {
                const next = variants.find((p) => p.id === e.target.value)!;
                const ids = new Set(next.controls.map((c) => c.id));
                onChange({
                  profile: next.id,
                  bindings: Object.fromEntries(
                    Object.entries(value.bindings).filter(([id]) =>
                      ids.has(id),
                    ),
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
          disabled={capturing || !Object.keys(value.bindings).length}
          onClick={() => {
            onChange({ profile: value.profile, bindings: {} });
            setPending(null);
            setMessage("Defaults restored.");
          }}
        >
          Reset to defaults
        </button>
      </div>
      <table className="controls-table" aria-label={`${profile.name} controls`}>
        <thead>
          <tr>
            <th scope="col">Button</th>
            <th scope="col">Action</th>
            <th scope="col">Keyboard</th>
            <th scope="col">Devices</th>
          </tr>
        </thead>
        <tbody>
          {profile.controls.map((item) => {
            const override = value.bindings[item.id];
            const keyLabel = keyName(override?.key ?? item.key);
            const active = selected === item.id;
            return (
              <tr
                key={item.id}
                className={active && (capturing || pending) ? "capturing" : ""}
              >
                <th scope="row">{item.label}</th>
                <td>
                  <input
                    className="action-input"
                    maxLength={32}
                    disabled={capturing}
                    value={override?.label ?? ""}
                    placeholder={item.label}
                    aria-label={`${item.label} action`}
                    onChange={(e) => patch(item.id, { label: e.target.value })}
                  />
                </td>
                <td>
                  <div className="control-key-field">
                    <button
                      ref={(node) => {
                        captureButtons.current[item.id] = node;
                      }}
                      type="button"
                      className="binding-button"
                      disabled={capturing}
                      aria-label={`${item.label} keyboard, ${
                        capturing && active
                          ? `press a key… ${seconds}`
                          : keyLabel
                      }`}
                      onClick={() => startCapture(item.id)}
                    >
                      {capturing && active ? (
                        `Press a key… ${seconds}`
                      ) : (
                        <>
                          <span className="binding-key">{keyLabel}</span>
                          <span className="binding-edit" aria-hidden="true">
                            Edit
                          </span>
                        </>
                      )}
                    </button>
                    {capturing && active && (
                      <button
                        type="button"
                        className="secondary"
                        onClick={cancel}
                      >
                        Cancel
                      </button>
                    )}
                    {pending && active && (
                      <div className="binding-conflict">
                        <span>
                          {keyName(pending.key)} also controls{" "}
                          {pending.conflicts.join(", ")}.
                        </span>
                        <button
                          type="button"
                          className="secondary"
                          onClick={() => {
                            patch(item.id, { key: pending.key });
                            setPending(null);
                            setMessage("Both controls use this key.");
                            focusSelected();
                          }}
                        >
                          Use for both
                        </button>
                        <button
                          type="button"
                          className="text-button"
                          onClick={cancel}
                        >
                          Cancel
                        </button>
                      </div>
                    )}
                  </div>
                </td>
                <td>
                  <details className="device-bindings">
                    <summary title="Device-specific defaults. Leave these blank to use the player's controller setup.">
                      Devices
                    </summary>
                    <div className="device-binding-fields">
                      <label>
                        Controller button
                        <input
                          type="number"
                          min={0}
                          max={63}
                          disabled={capturing}
                          value={override?.button ?? ""}
                          onChange={(e) =>
                            patch(item.id, {
                              button: e.target.value || undefined,
                            })
                          }
                        />
                      </label>
                      <label>
                        Controller axis
                        <select
                          disabled={capturing}
                          value={override?.axis ?? ""}
                          onChange={(e) =>
                            patch(item.id, {
                              axis: e.target.value || undefined,
                            })
                          }
                        >
                          <option value="">Automatic</option>
                          {Array.from({ length: 16 }, (_, i) =>
                            ["-", "+"].map((sign) => (
                              <option key={`${sign}${i}`} value={`${sign}${i}`}>
                                Axis {i} {sign}
                              </option>
                            )),
                          )}
                        </select>
                      </label>
                      <label>
                        Mouse button
                        <select
                          disabled={capturing}
                          value={override?.mouse ?? ""}
                          onChange={(e) =>
                            patch(item.id, {
                              mouse: e.target.value
                                ? Number(e.target.value)
                                : undefined,
                            })
                          }
                        >
                          <option value="">None</option>
                          {[
                            "Left",
                            "Right",
                            "Middle",
                            "Button 4",
                            "Button 5",
                          ].map((name, i) => (
                            <option key={i} value={i + 1}>
                              {name}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>
                  </details>
                </td>
              </tr>
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
