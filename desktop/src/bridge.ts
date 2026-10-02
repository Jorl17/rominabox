import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Controls } from "./controls";
import type { Hotkeys, Refusal } from "./hotkeys";
import { SHADER_EXTENSIONS, baseName } from "./shaderFiles";

export const native = isTauri();
export type GameInfo = {
  title: string;
  system: string;
  filename: string;
  size: number;
  source: string;
  matched: boolean;
  catalogName?: string;
  iconPath?: string;
  warnings: string[];
};
export type Picture = { path: string; url: string };
export type ExportRequest = {
  /** Mac and Windows at once, in one zip. */
  bothPlatforms?: boolean;
  rom: string;
  title: string;
  system: string;
  icon: string | null;
  background: string | null;
  /** The background picture drawn in the palette's screen colour. */
  tintBackground: boolean;
  showMenu: boolean;
  includeAchievements: boolean;
  startAtMenu: boolean;
  splash: boolean;
  keepPlayingInBackground: boolean;
  autosaveOnQuit: boolean;
  /** Every connected pad plays as player 1. */
  everyPadIsPlayerOne: boolean;
  advancedEmulatorAccess: boolean;
  /** A Mac app also runs on Intel Macs. */
  intelMacs: boolean;
  shaders?: {
    bundled: string[];
    custom: { name: string; path: string }[];
    initial?: string | null;
  };
  firmware: string[];
  theme: string;
  palette: string;
  menuSounds: string;
  controls: Controls;
  hotkeys: Hotkeys;
  /** The Options entries of the game, or null for those of the design. */
  menuEntries?: string[] | null;
  outputDir: string;
  /** Replace an app already at the destination. Without it we do nothing in
   * the export and fail with `AppExists`. */
  replace?: boolean;
  target: ExportTarget | null;
};
/** The platform of an export, by its name in the backend. */
export type ExportTarget = "macos" | "windows";
/** Our messages about cores in the export, when we fetch any or cannot. */
export type CoreActivity =
  | { kind: "fetching"; downloading: number; updating: number }
  | { kind: "failed"; missing: number };
export type ExportProgress = {
  stage: string;
  fraction: number;
  message: string;
  cores?: CoreActivity;
};
/** A required core is not cached, and we could not download it. */
export class CoreDownloadFailed extends Error {}
/** The stage of that failure in the exporter (`ErrorStage::Cores`). */
const CORES_STAGE = "cores";
/** An app is already where this one would go. We did nothing. */
export class AppExists extends Error {
  constructor(
    sentence: string,
    readonly app: string,
    readonly folder: string,
  ) {
    super(sentence);
  }
}
/** `ErrorStage::Exists`. */
const EXISTS_STAGE = "exists";
export type ExportResult = {
  appPath: string;
  installedBytes: number;
  runtimeBytes: number;
  contentBytes: number;
};
export async function pickFile(kind: "game" | "image"): Promise<string | null> {
  const path = await open({
    multiple: false,
    directory: false,
    title: kind === "game" ? "Choose a game" : "Choose an image",
    ...(kind === "image"
      ? {
          filters: [
            {
              name: "Images",
              extensions: [
                "png",
                "jpg",
                "jpeg",
                "webp",
                "gif",
                "bmp",
                "tif",
                "tiff",
                "ico",
              ],
            },
          ],
        }
      : {}),
  });
  return typeof path === "string" ? path : null;
}
export async function pickShader(): Promise<string | null> {
  const path = await open({
    multiple: false,
    directory: false,
    title: "Add a shader",
    filters: [{ name: "Shaders", extensions: SHADER_EXTENSIONS }],
  });
  return typeof path === "string" ? path : null;
}
export async function pickFolder(): Promise<string | null> {
  const path = await open({
    directory: true,
    multiple: false,
    title: "Save your game app",
  });
  return typeof path === "string" ? path : null;
}
export function inspectGame(
  path: string,
  online: boolean,
  systemOverride?: string,
): Promise<GameInfo> {
  return invoke("inspect_game", { path, online, systemOverride });
}
// The name of an added shader file, from the engine, as for one without a
// name in a command-line request. The browser walkthrough has no engine, so
// there we show the name of the file.
export function customShaderName(path: string): Promise<string> {
  if (!native) return Promise.resolve(baseName(path));
  return invoke("custom_shader_name", { path });
}
export type ShaderWarning = { path: string; sentence: string };
// The browser walkthrough has no desktop shell, so we show none.
export function shaderWarnings(selection: {
  bundled: string[];
  custom: { name: string; path: string }[];
  initial: string | null;
}): Promise<ShaderWarning[]> {
  if (!native) return Promise.resolve([]);
  return invoke("shader_warnings", { selection });
}
export type Traveling = { entry: string; files: string[] };
/**
 * The browser walkthrough has no desktop shell. In its server we answer
 * `question` with the same CLI command as in the shell, because with a second
 * copy of a rule in the page, the page and the export could disagree.
 */
function askWalkthrough<T>(
  question: string,
  query: URLSearchParams,
  unread: string,
): Promise<T> {
  return fetch(`/__rominabox/${question}?${query}`).then(async (response) => {
    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as {
        message?: string;
      } | null;
      // We show this sentence on the page. Replacing it would hide the reason
      // from the exporter and make the drop look like a one-file game.
      const message =
        body && typeof body.message === "string" && body.message
          ? body.message
          : unread;
      throw new Error(message);
    }
    return response.json() as Promise<T>;
  });
}
// We ask with the console, as in the export, because a companion required
// for one console is optional for another. Without it, the Also importing
// line could list files that we do not copy in the export.
export function travelingFiles(
  path: string,
  system: string,
): Promise<Traveling> {
  if (native) return invoke("traveling_files", { path, system });
  return askWalkthrough(
    "traveling",
    new URLSearchParams({ path, system }),
    "The files that travel with this game could not be read.",
  );
}
export const FirmwareNoticeKind = {
  Required: "required",
  Duplicate: "duplicate",
  Unmatched: "unmatched",
  Optional: "optional",
  Ready: "ready",
} as const;
export type FirmwareNoticeKind =
  (typeof FirmwareNoticeKind)[keyof typeof FirmwareNoticeKind];
export type FirmwareAssessment = {
  canContinue: boolean;
  notices: { kind: FirmwareNoticeKind; text: string }[];
  files: { name: string; counted: boolean; reason: string | null }[];
};
export function assessFirmware(
  system: string,
  files: string[],
): Promise<FirmwareAssessment> {
  if (native) return invoke("assess_firmware", { system, files });
  const query = new URLSearchParams({ system });
  for (const file of files) query.append("file", file);
  return askWalkthrough(
    "firmware",
    query,
    "What this console needs could not be read.",
  );
}
/** The rule that `hotkeys` would break in the game's menu, with the game's
 * `controls` on `system`, as we would reject it in the export, or null. We
 * check none in the browser preview. */
export function checkHotkeys(
  hotkeys: Hotkeys,
  system: string,
  controls: Controls,
): Promise<Refusal | null> {
  if (!native) return Promise.resolve(null);
  return invoke("check_hotkeys", { hotkeys, system, controls }).then(
    () => null,
    (refusal: Refusal) => refusal,
  );
}
/**
 * Wait up to `seconds` for a press on a controller, a button or a moved stick
 * or trigger, and return its pad position. Return null on a timeout, when the
 * wait was cancelled, or when we cannot read a controller here (we read none
 * in the browser preview).
 */
export function capturePadPosition(seconds: number): Promise<string | null> {
  if (!native) return Promise.resolve(null);
  return invoke("capture_pad_position", { seconds });
}
export function cancelPadCapture(): Promise<void> {
  return native ? invoke("cancel_pad_capture") : Promise.resolve();
}
export async function pickFirmware(): Promise<string[]> {
  const files = await open({
    multiple: true,
    directory: false,
    title: "Choose BIOS files",
  });
  return Array.isArray(files) ? files : files ? [files] : [];
}
export async function readImage(path: string): Promise<Picture> {
  const bytes = await invoke<number[]>("image_preview", { path });
  return {
    path,
    url: URL.createObjectURL(
      new Blob([new Uint8Array(bytes)], { type: "image/png" }),
    ),
  };
}
export async function menuPreview(
  background: string | null,
  tintBackground: boolean,
  palette: string,
  design: string,
): Promise<string> {
  const bytes = await invoke<number[]>("menu_preview", {
    background,
    tintBackground,
    palette,
    design,
  });
  return URL.createObjectURL(
    new Blob([new Uint8Array(bytes)], { type: "image/png" }),
  );
}
/** Export a game. For a request with both platforms we make it for Mac and
 * Windows, in one zip, and we keep that option out of the game. */
export async function exportGame(
  request: ExportRequest,
): Promise<ExportResult> {
  try {
    return await invoke<ExportResult>("export_game", { request });
  } catch (reason) {
    throw exportFailure(reason);
  }
}
/**
 * The `{stage, sentence}` from the exporter (`export_error::AuthorError`). We
 * write the sentence in the exporter, and here we only pick the failures that
 * have separate controls in the builder.
 */
export function exportFailure(reason: unknown): unknown {
  if (
    typeof reason !== "object" ||
    reason === null ||
    !("stage" in reason) ||
    !("sentence" in reason)
  )
    return reason;
  const { stage, sentence, existing } = reason as {
    stage: string;
    sentence: string;
    existing?: { name: string; folder: string };
  };
  if (stage === CORES_STAGE) return new CoreDownloadFailed(sentence);
  if (stage === EXISTS_STAGE && existing)
    return new AppExists(sentence, existing.name, existing.folder);
  return new Error(sentence);
}
export function cancelExport(): Promise<void> {
  return invoke("cancel_export");
}
/** The platform we export for on this machine, or null where we cannot. */
export function exportTarget(): Promise<ExportTarget | null> {
  return invoke("export_target");
}
export function defaultDestination(): Promise<string> {
  return invoke("default_destination");
}
export function reveal(path: string): Promise<void> {
  return revealItemInDir(path);
}
export function onExportProgress(callback: (value: ExportProgress) => void) {
  return listen<ExportProgress>("export-progress", (e) => callback(e.payload));
}
export function onNativeDrop(
  callback: (paths: string[], position: { x: number; y: number }) => void,
  hover: (value: boolean) => void,
) {
  return getCurrentWindow().onDragDropEvent(async (e) => {
    if (e.payload.type === "drop") {
      hover(false);
      const scale = await getCurrentWindow().scaleFactor();
      callback(e.payload.paths, {
        x: e.payload.position.x / scale,
        y: e.payload.position.y / scale,
      });
    } else hover(e.payload.type === "over" || e.payload.type === "enter");
  });
}

export type ProjectArchiveResult = {
  archivePath: string;
  archiveBytes: number;
};
export type OpenProject = {
  settings: Omit<ExportRequest, "outputDir">;
  extractionDir: string;
};
export async function pickProjectSave(title: string): Promise<string | null> {
  return save({
    title: "Save project — includes game and artwork",
    defaultPath: `${title.replace(/[\\/:*?"<>|]/g, "-")}.rominabox`,
    filters: [
      {
        name: "ROM-in-a-Box project (includes ROM)",
        extensions: ["rominabox"],
      },
    ],
  });
}
export async function pickProjectOpen(): Promise<string | null> {
  const path = await open({
    title: "Open project",
    multiple: false,
    directory: false,
    filters: [{ name: "ROM-in-a-Box project", extensions: ["rominabox"] }],
  });
  return typeof path === "string" ? path : null;
}
export function saveProject(
  archivePath: string,
  settings: ExportRequest,
): Promise<ProjectArchiveResult> {
  return invoke("save_project", { request: { archivePath, settings } });
}
export function openProject(archivePath: string): Promise<OpenProject> {
  return invoke("open_project", { archivePath });
}
