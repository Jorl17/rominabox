import { useEffect, useState } from "react";
import * as bridge from "./bridge";

type CustomShader = { name: string; path: string };

/** The author's filters that a Windows game may fail to load, by path, from
 * the shader check in the shell, because a file would be too deep to open. */
export function useShaderWarnings(
  bundled: string[],
  custom: CustomShader[],
  initial: string | null,
): Record<string, string> {
  const [warnings, setWarnings] = useState<Record<string, string>>({});
  // We ask again when the selection changes, not for each new array.
  const selection = JSON.stringify({ bundled, custom, initial });
  useEffect(() => {
    let current = true;
    const asked = JSON.parse(selection) as Parameters<typeof bridge.shaderWarnings>[0];
    if (asked.custom.length === 0) {
      setWarnings({});
      return;
    }
    bridge
      .shaderWarnings(asked)
      .then((found) => {
        if (current)
          setWarnings(Object.fromEntries(found.map((warning) => [warning.path, warning.sentence])));
      })
      // When we reject a selection here, we reject it again, with the reason,
      // when we make the game.
      .catch(() => {
        if (current) setWarnings({});
      });
    return () => {
      current = false;
    };
  }, [selection]);
  return warnings;
}

/** An author's filter among the chosen ones: a click removes it. */
export function CustomShaderCard({
  shader,
  warning,
  onRemove,
}: {
  shader: CustomShader;
  warning?: string;
  onRemove: () => void;
}) {
  return (
    <button type="button" className="shader-card chosen" aria-pressed={true} onClick={onRemove}>
      <span className="shader-name">{shader.name}</span>
      {warning && <small className="shader-warning">{warning}</small>}
    </button>
  );
}
