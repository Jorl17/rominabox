import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { ChevronRight, Plus } from "lucide-react";
import * as bridge from "./bridge";
import { CustomShaderCards } from "./customShaders";
import shaderCatalog from "../../integrations/shaders/catalog.json";
import {
  NOT_A_SHADER_FILE,
  SHADER_ACCEPT,
  SHADER_FORMATS,
  isShaderFile,
} from "./shaderFiles";

// The same pictures as in the exported game, rendered from the GLSL of each
// shader with scripts/render_shader_previews.py. We read them as a directory
// and do not list them, so a preset added to the catalogue comes with its
// preview instead of a broken image.
const shaderPreviews = Object.fromEntries(
  Object.entries(
    import.meta.glob("../../integrations/shaders/previews/*.png", {
      eager: true,
      query: "?url",
      import: "default",
    }) as Record<string, string>,
  ).map(([path, url]) => [
    path
      .split("/")
      .pop()!
      .replace(/\.png$/, ""),
    url,
  ]),
);

/** A game without filters. */
export const NO_SHADERS: bridge.ShaderSelection = {
  bundled: [],
  custom: [],
  initial: null,
};

type Change = Dispatch<SetStateAction<bridge.ShaderSelection>>;

/** Add the shader file at `filePath` to the filters of the author, once. */
export async function addCustomShader(filePath: string, onChange: Change) {
  if (!isShaderFile(filePath)) throw new Error(NOT_A_SHADER_FILE);
  const name = await bridge.customShaderName(filePath);
  onChange((current) =>
    current.custom.some((item) => item.path === filePath)
      ? current
      : { ...current, custom: [...current.custom, { name, path: filePath }] },
  );
}

/** The filters we recommend for a game of the console `system`, whose name
 * in the No-Intro catalog is `catalogName` when we identified it. */
function useRecommendedFilters(system: string, catalogName?: string) {
  const [filters, setFilters] = useState<string[]>([]);
  useEffect(() => {
    let current = true;
    setFilters([]);
    bridge
      .recommendedFilters(system, catalogName)
      .then((found) => {
        if (current) setFilters(found);
      })
      // Without the list we recommend nothing, and the author can still
      // choose every filter.
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [system, catalogName]);
  return filters;
}

/** The Picture filters section of the Menu step: the presets of the shader
 * catalog and the files of the author, which the player can choose between
 * in the game, and the filter at the start of the game. With "Add
 * recommended", the author adds the filters we recommend for the game to
 * the ones already chosen. */
export function PictureFilters({
  shaders,
  onChange,
  system,
  catalogName,
  onError,
  clearError,
}: {
  shaders: bridge.ShaderSelection;
  onChange: Change;
  system: string;
  catalogName?: string;
  onError: (reason: unknown) => void;
  clearError: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const { bundled, custom, initial } = shaders;
  const count = bundled.length + custom.length;
  const recommended = useRecommendedFilters(system, catalogName);
  function addRecommended() {
    onChange((current) => ({
      ...current,
      bundled: [
        ...current.bundled,
        ...recommended.filter((id) => !current.bundled.includes(id)),
      ],
    }));
  }
  function togglePreset(id: string) {
    onChange((current) =>
      current.bundled.includes(id)
        ? {
            ...current,
            bundled: current.bundled.filter((item) => item !== id),
            initial: current.initial === id ? null : current.initial,
          }
        : { ...current, bundled: [...current.bundled, id] },
    );
  }
  function removeCustomShader(shader: bridge.CustomShader) {
    onChange((current) => ({
      ...current,
      custom: current.custom.filter((item) => item.path !== shader.path),
      initial: current.initial === shader.name ? null : current.initial,
    }));
  }
  async function chooseShaderFile() {
    clearError();
    if (!bridge.native) {
      input.current?.click();
      return;
    }
    try {
      const path = await bridge.pickShader();
      if (path) await addCustomShader(path, onChange);
    } catch (e) {
      onError(e);
    }
  }
  return (
    <details className="advanced picture-filters">
      <summary>
        <ChevronRight size={16} />
        {count === 0
          ? "Picture filters (shaders) · none selected"
          : `Picture filters (shaders) · ${count} selected`}
      </summary>
      <div className="shader-choices">
        <div className="shader-intro">
          <p className="shader-lede">
            Pick the shaders players can choose in the game. Click one to select
            it.
          </p>
          {recommended.length > 0 && (
            <button
              type="button"
              className="secondary"
              disabled={recommended.every((id) => bundled.includes(id))}
              onClick={addRecommended}
            >
              Add recommended
            </button>
          )}
        </div>
        <div className="shader-grid">
          {shaderCatalog.presets.map((preset) => {
            const chosen = bundled.includes(preset.id);
            return (
              <button
                key={preset.id}
                type="button"
                className={`shader-card${chosen ? " chosen" : ""}`}
                aria-pressed={chosen}
                onClick={() => togglePreset(preset.id)}
              >
                <img
                  className="shader-preview"
                  src={shaderPreviews[preset.id]}
                  alt=""
                />
                <span className="shader-name">{preset.name}</span>
                <span className="shader-detail">{preset.detail}</span>
              </button>
            );
          })}
          <CustomShaderCards
            selection={[bundled, custom, initial]}
            onRemove={removeCustomShader}
          />
          <button
            type="button"
            className="shader-card shader-add"
            data-drop="shader"
            onClick={() => {
              void chooseShaderFile();
            }}
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              event.preventDefault();
              const file = event.dataTransfer.files[0];
              if (!file) return;
              const dropped = file as File & { path?: string };
              addCustomShader(dropped.path || file.name, onChange).catch(
                onError,
              );
            }}
          >
            <span>
              <Plus size={22} aria-hidden="true" />
              Add your own
              <small className="shader-formats">
                {SHADER_FORMATS.join(" ")}
              </small>
            </span>
          </button>
        </div>
        <input
          ref={input}
          type="file"
          hidden
          accept={SHADER_ACCEPT}
          data-shader
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) addCustomShader(file.name, onChange).catch(onError);
            event.target.value = "";
          }}
        />
        {count > 0 && (
          <label className="shader-initial">
            Starts on
            <select
              aria-label="Starts on"
              value={initial ?? ""}
              onChange={(event) => {
                const value = event.target.value || null;
                onChange((current) => ({ ...current, initial: value }));
              }}
            >
              <option value="">Unfiltered</option>
              {shaderCatalog.presets
                .filter((preset) => bundled.includes(preset.id))
                .map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.name}
                  </option>
                ))}
              {custom.map((shader) => (
                <option key={shader.path} value={shader.name}>
                  {shader.name}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
    </details>
  );
}
