import { useEffect, useState } from "react";
import { TriangleAlert } from "lucide-react";
import * as bridge from "./bridge";
import { ShaderWarning } from "./ShaderWarning";

type CustomShader = { name: string; path: string };

const NONE: bridge.ShaderWarnings = { warned: [], warnings: [] };

/** The warnings about the author's filters, from the shader check in the
 * shell. */
function useShaderWarnings(
  bundled: string[],
  custom: CustomShader[],
  initial: string | null,
): bridge.ShaderWarnings {
  const [warnings, setWarnings] = useState(NONE);
  // We ask again when the selection changes, not for each new array.
  const selection = JSON.stringify({ bundled, custom, initial });
  useEffect(() => {
    let current = true;
    const asked = JSON.parse(selection) as Parameters<
      typeof bridge.shaderWarnings
    >[0];
    if (asked.custom.length === 0) {
      setWarnings(NONE);
      return;
    }
    bridge
      .shaderWarnings(asked)
      .then((found) => {
        if (current) setWarnings(found);
      })
      // When we reject a selection here, we reject it again, with the reason,
      // when we make the game.
      .catch(() => {
        if (current) setWarnings(NONE);
      });
    return () => {
      current = false;
    };
  }, [selection]);
  return warnings;
}

/** The author's filters among the chosen ones, each marked when it will
 * not load on a platform. Clicking one removes it. After the cards, across
 * the whole grid, we write the warnings, each of which can be about several
 * filters. */
export function CustomShaderCards({
  selection: [bundled, custom, initial],
  onRemove,
}: {
  selection: [string[], CustomShader[], string | null];
  onRemove: (shader: CustomShader) => void;
}) {
  const { warned, warnings } = useShaderWarnings(bundled, custom, initial);
  return (
    <>
      {custom.map((shader) => (
        <CustomShaderCard
          key={shader.path}
          shader={shader}
          warned={warned.includes(shader.path)}
          onRemove={() => onRemove(shader)}
        />
      ))}
      {warnings.length > 0 && (
        <div className="shader-warnings" role="note">
          {warnings.map((warning) => (
            <ShaderWarning key={warning.text} warning={warning} />
          ))}
        </div>
      )}
    </>
  );
}

/** An author's filter among the chosen ones: a click removes it. */
function CustomShaderCard({
  shader,
  warned,
  onRemove,
}: {
  shader: CustomShader;
  warned: boolean;
  onRemove: () => void;
}) {
  return (
    <button
      type="button"
      className="shader-card chosen"
      aria-pressed={true}
      onClick={onRemove}
    >
      <span className="shader-name">
        {shader.name}
        {warned && (
          <TriangleAlert
            className="shader-warned"
            size={14}
            aria-label="Will not load on every platform"
          />
        )}
      </span>
    </button>
  );
}
