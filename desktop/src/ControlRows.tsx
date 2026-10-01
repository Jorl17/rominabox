import type { ReactNode } from "react";
import type { Capture } from "./controlCapture";
import { keyName } from "./keys";
import { positionName, positionOf } from "./padPositions";

type Control = { id: string; label: string; key: string };
type Binding = { label?: string; key?: string; pad?: string };

/** The values a row uses from the editor. */
export type RowContext = {
  bindings: Record<string, Binding | undefined>;
  capture: Capture | null;
  pending: { key: string; conflicts: string[] } | null;
  /** Why a control would get no input in the game, when it would. */
  stops: Record<string, string>;
  busy: boolean;
  bind: (stop: string, members: string[]) => void;
  cancel: () => void;
  useForBoth: () => void;
  label: (id: string, label: string) => void;
  /** Registers a Bind button, so focus can come back to it. */
  button: (stop: string, node: HTMLButtonElement | null) => void;
  devices: (control: Control) => ReactNode;
};

/** A control's binding in the words the game shows: its key, and where on the pad it is. */
function Binding({
  control,
  context,
}: {
  control: Control;
  context: RowContext;
}) {
  const binding = context.bindings[control.id];
  return (
    <>
      <span className="binding-key">
        {keyName(binding?.key ?? control.key)}
      </span>
      <span className="binding-pad">
        {positionName(positionOf(control, context.bindings))}
      </span>
    </>
  );
}

function waitingFor(control: Control, context: RowContext) {
  const capture = context.capture;
  return !!capture && capture.members[capture.step] === control.id;
}

function Countdown({ context }: { context: RowContext }) {
  return <>Press an input… {context.capture?.seconds}</>;
}

function CancelButton({ context }: { context: RowContext }) {
  return (
    <button type="button" className="secondary" onClick={context.cancel}>
      Cancel
    </button>
  );
}

/** The key another control already uses, and the choice to share it. */
function Conflict({ context }: { context: RowContext }) {
  if (!context.pending) return null;
  return (
    <div className="binding-conflict">
      <span>
        {keyName(context.pending.key)} also controls{" "}
        {context.pending.conflicts.join(", ")}.
      </span>
      <button type="button" className="secondary" onClick={context.useForBoth}>
        Use for both
      </button>
      <button type="button" className="text-button" onClick={context.cancel}>
        Cancel
      </button>
    </div>
  );
}

function Stops({
  control,
  context,
}: {
  control: Control;
  context: RowContext;
}) {
  const why = context.stops[control.id];
  return why ? <span className="binding-stops">{why}</span> : null;
}

function LabelField({
  control,
  context,
}: {
  control: Control;
  context: RowContext;
}) {
  return (
    <input
      className="action-input"
      maxLength={32}
      disabled={context.busy}
      value={context.bindings[control.id]?.label ?? ""}
      placeholder={control.label}
      aria-label={`${control.label} action label`}
      onChange={(e) => context.label(control.id, e.target.value)}
    />
  );
}

function Devices({
  control,
  context,
}: {
  control: Control;
  context: RowContext;
}) {
  return (
    <details className="device-bindings">
      <summary title="Where on the pad this control is, and the mouse button that also works it.">
        Devices
      </summary>
      <div className="device-binding-fields">{context.devices(control)}</div>
    </details>
  );
}

/** One control, bound by its own Bind button. */
export function ControlRow({
  control,
  context,
}: {
  control: Control;
  context: RowContext;
}) {
  const waiting = waitingFor(control, context);
  const binding = context.bindings[control.id];
  const words = `${keyName(binding?.key ?? control.key)}, ${positionName(
    positionOf(control, context.bindings),
  )}`;
  return (
    <tr className={waiting ? "capturing" : ""}>
      <th scope="row">{control.label}</th>
      <td>
        <LabelField control={control} context={context} />
      </td>
      <td>
        <div className="control-key-field">
          <button
            ref={(node) => context.button(control.id, node)}
            type="button"
            className="binding-button"
            disabled={context.busy}
            aria-label={`${control.label} binding, ${
              waiting ? `press an input… ${context.capture?.seconds}` : words
            }`}
            onClick={() => context.bind(control.id, [control.id])}
          >
            {waiting && !context.pending ? (
              <Countdown context={context} />
            ) : (
              <>
                <Binding control={control} context={context} />
                <span className="binding-edit" aria-hidden="true">
                  Bind
                </span>
              </>
            )}
          </button>
          {waiting && !context.pending && <CancelButton context={context} />}
          {waiting && <Conflict context={context} />}
          <Stops control={control} context={context} />
        </div>
      </td>
      <td>
        <Devices control={control} context={context} />
      </td>
    </tr>
  );
}

/**
 * A stick: one Bind for all its directions, which we capture in order as in
 * the game, with a mark on the direction we are waiting for. Each direction
 * is still a row, for its label and its position on the pad.
 */
export function StickRows({
  group,
  title,
  members,
  context,
}: {
  group: string;
  /** The name of the stick on the pad. */
  title: string;
  members: Control[];
  context: RowContext;
}) {
  const capture = context.capture;
  const binding = !!capture && capture.stop === group;
  return (
    <>
      <tr className={`stick-row${binding ? " capturing" : ""}`}>
        <th scope="rowgroup" colSpan={2}>
          {title}
        </th>
        <td>
          <div className="control-key-field">
            <button
              ref={(node) => context.button(group, node)}
              type="button"
              className="binding-button stick-bind"
              disabled={context.busy}
              aria-label={`Bind ${title}, one direction after another`}
              onClick={() =>
                context.bind(
                  group,
                  members.map((member) => member.id),
                )
              }
            >
              {binding
                ? `${title}: ${capture.step + 1} of ${members.length}`
                : "Bind stick"}
            </button>
            {binding && !context.pending && <CancelButton context={context} />}
          </div>
        </td>
        <td />
      </tr>
      {members.map((member) => {
        const waiting = waitingFor(member, context);
        return (
          <tr
            key={member.id}
            className={`stick-member${waiting ? " capturing" : ""}`}
          >
            <th scope="row">{member.label}</th>
            <td>
              <LabelField control={member} context={context} />
            </td>
            <td>
              <div className="control-key-field">
                <span className="binding-text" aria-live="polite">
                  {waiting && !context.pending ? (
                    <Countdown context={context} />
                  ) : (
                    <Binding control={member} context={context} />
                  )}
                </span>
                {waiting && <Conflict context={context} />}
                <Stops control={member} context={context} />
              </div>
            </td>
            <td>
              <Devices control={member} context={context} />
            </td>
          </tr>
        );
      })}
    </>
  );
}
