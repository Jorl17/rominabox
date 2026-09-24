import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Controls } from "./controls";

export const native = isTauri();
export type GameInfo = {
  title: string;
  system: string;
  filename: string;
  size: number;
  source: string;
  matched: boolean;
  catalogName?: string;
  description?: string;
  iconPath?: string;
  warnings: string[];
};
export type Picture = { path: string; url: string };
export type ExportRequest = {
  rom: string;
  title: string;
  system: string;
  description: string;
  icon: string | null;
  background: string | null;
  showMenu: boolean;
  includeAchievements: boolean;
  startAtMenu: boolean;
  splash: boolean;
  keepPlayingInBackground: boolean;
  autosaveOnQuit: boolean;
  advancedEmulatorAccess: boolean;
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
  outputDir: string;
  target: string;
};
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
    filters: [{ name: "Shaders", extensions: ["glsl", "glslp"] }],
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
export type Traveling = { entry: string; files: string[] };
// We ask with the console, as in the export, because a companion required
// for one console is optional for another. Without it, the Also importing
// line could list files that we do not copy in the export.
export function travelingFiles(
  path: string,
  system: string,
): Promise<Traveling> {
  // The browser walkthrough has no desktop shell, but we must still use
  // content::collect, because with a second copy of that rule in the page,
  // the Also importing line and the export could list different files. In the
  // walkthrough server we run the same command as in the shell.
  if (native) return invoke("traveling_files", { path, system });
  const query = new URLSearchParams({ path, system });
  return fetch(`/__rominabox/traveling?${query}`).then(async (response) => {
    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as {
        message?: string;
      } | null;
      // We show this sentence on the page. Replacing it would hide the reason
      // from the exporter and make the drop look like a one-file game.
      const message =
        body && typeof body.message === "string" && body.message
          ? body.message
          : "The files that travel with this game could not be read.";
      throw new Error(message);
    }
    return response.json() as Promise<Traveling>;
  });
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
  return invoke("assess_firmware", { system, files });
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
  palette: string,
  design: string,
): Promise<string> {
  const bytes = await invoke<number[]>("menu_preview", {
    background,
    palette,
    design,
  });
  return URL.createObjectURL(
    new Blob([new Uint8Array(bytes)], { type: "image/png" }),
  );
}
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
  const { stage, sentence } = reason as { stage: string; sentence: string };
  if (stage === CORES_STAGE) return new CoreDownloadFailed(sentence);
  return new Error(sentence);
}
export function cancelExport(): Promise<void> {
  return invoke("cancel_export");
}
export function availableSystems(): Promise<string[]> {
  return invoke("available_systems");
}
export function ensureCores(): Promise<unknown> {
  return invoke("ensure_cores");
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
