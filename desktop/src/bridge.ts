import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
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
  fastForward: boolean;
  fastForwardSpeed: number;
  fastForwardHold: boolean;
  autosaveOnQuit: boolean;
  /** Every connected pad plays as player 1. */
  everyPadIsPlayerOne: boolean;
  advancedEmulatorAccess: boolean;
  /** A Mac app also runs on Intel Macs. */
  intelMacs: boolean;
  /** What the author left out of, and added to, the files of the game. */
  files: GameFiles;
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
/** A failure that comes from a bug in ROM-in-a-Box, with the technical
 * details for a bug report. */
export class ExportBug extends Error {
  constructor(
    sentence: string,
    readonly details: string,
  ) {
    super(sentence);
  }
}
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
  files?: GameFiles,
): Promise<GameInfo> {
  return invoke("inspect_game", { path, online, systemOverride, files });
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
/** What a file is to the game (content::FileRole). */
export type FileRole =
  | { kind: "game" }
  | { kind: "named" }
  | { kind: "companion"; required: boolean }
  | { kind: "added" };
export type TravelingFile = { name: string; role: FileRole };
/**
 * What the author left out of, and added to, the files of a game, and
 * whether they chose to decompress a compressed disc to include its patches.
 */
export type GameFiles = {
  leftOut: string[];
  added: string[];
  decompress?: boolean;
};
/** Patches for a compressed disc, and the game's size without and with. */
export type Compressed = {
  game: string;
  patches: string[];
  withoutBytes: number;
  withBytes: number;
  included: boolean;
};
export type Traveling = {
  entry: string;
  files: TravelingFile[];
  patches: string[];
  refused: string[];
  added: string[];
  compressed?: Compressed | null;
};
/** The game file, the files copied with it, then the patches applied to it. */
export function travelingNames(traveling: Traveling): string[] {
  return [...traveling.files.map((file) => file.name), ...traveling.patches];
}
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
  files?: GameFiles,
): Promise<Traveling> {
  if (native) return invoke("traveling_files", { path, system, files });
  return askWalkthrough(
    "traveling",
    new URLSearchParams({ path, system, files: JSON.stringify(files ?? null) }),
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
/** Files the author adds to a game: companions, patches, anything else. */
export async function pickGameFiles(): Promise<string[]> {
  const files = await open({
    multiple: true,
    directory: false,
    title: "Add files to the game",
  });
  return Array.isArray(files) ? files : files ? [files] : [];
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
  const { stage, sentence, existing, bug } = reason as {
    stage: string;
    sentence: string;
    existing?: { name: string; folder: string };
    bug?: string;
  };
  if (stage === CORES_STAGE) return new CoreDownloadFailed(sentence);
  if (stage === EXISTS_STAGE && existing)
    return new AppExists(sentence, existing.name, existing.folder);
  if (bug) return new ExportBug(sentence, bug);
  return new Error(sentence);
}
export function cancelExport(): Promise<void> {
  return invoke("cancel_export");
}
/** The platform we export for on this machine, or null where we cannot. */
export function exportTarget(): Promise<ExportTarget | null> {
  return invoke("export_target");
}
/** Whether a new game also runs on Intel Macs: true for a builder on an
 * Intel Mac. In the browser preview it is null, and the declared default
 * applies. */
export function intelMacsDefault(): Promise<boolean | null> {
  if (!native) return Promise.resolve(null);
  return invoke("intel_macs_default");
}
export function defaultDestination(): Promise<string> {
  return invoke("default_destination");
}
export function reveal(path: string): Promise<void> {
  return revealItemInDir(path);
}
/** Call `callback` when someone chooses About in the macOS menu bar. */
export function onAboutRequested(callback: () => void) {
  return listen("about-requested", () => callback());
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
/** One component of the builder and its games, as listed in the bundled
 * licence index. `file` is the name of its licence text. */
export type Component = {
  group:
    "native" | "cores" | "data" | "fonts" | "crates" | "npm" | "toolchains";
  title: string;
  version: string;
  licence: string;
  file: string;
  /** A copyright notice to show under the title, for ROM-in-a-Box itself. */
  copyright?: string;
};
/** Every component, for the About dialog: the bundled index in the app, the
 * repository's in the browser preview. */
export function aboutComponents(): Promise<Component[]> {
  if (native) return invoke("about_components");
  return fetch("/licenses/index.json").then((response) =>
    response.ok ? response.json() : [],
  );
}
export function licenceText(file: string): Promise<string> {
  if (native) return invoke("licence_text", { file });
  return fetch(`/licenses/${file}`).then((response) => response.text());
}
/** The builder's version; empty in the browser preview. */
export function appVersion(): Promise<string> {
  return native ? getVersion() : Promise.resolve("");
}
/** ROM-in-a-Box's web address, in the browser. */
export function openBugReport(): Promise<void> {
  if (native) return invoke("open_bug_report");
  window.open(__BUG_REPORTS__, "_blank", "noopener");
  return Promise.resolve();
}
export function openWebsite(): Promise<void> {
  if (native) return invoke("open_website");
  window.open(__WEBSITE__, "_blank", "noopener");
  return Promise.resolve();
}
