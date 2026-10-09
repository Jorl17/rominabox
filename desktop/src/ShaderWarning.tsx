import type { Warning } from "./bridge";
import { Help } from "./Help";

/** What will happen to shaders that will not load, with the help button
 * beside it. In its tooltip we say why and what to do about it, so the step
 * and the pop-up stay short. */
export function ShaderWarning({ warning }: { warning: Warning }) {
  return (
    <p className="shader-warning">
      {warning.text}
      <Help label="Why">{warning.detail}</Help>
    </p>
  );
}
