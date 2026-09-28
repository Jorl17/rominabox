import React, { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  ChevronRight,
  FileImage,
  FolderOpen,
  LoaderCircle,
  Plus,
  Save,
  TriangleAlert,
  X,
} from "lucide-react";
import { SYSTEMS, formatBytes, inspectRom } from "./inspection";
import * as bridge from "./bridge";
import { canExport, whyNot } from "./consoles";
import {
  afterExport,
  afterProgress,
  CoreFetchNotice,
  type CoreNotice,
} from "./CoreFetchNotice";
import designs from "../designs.json";
import declared from "../defaults.json";
import { ControlsEditor, emptyControls, type Controls } from "./controls";
import { Help, Checkbox } from "./Help";
import { MenuSoundPreview } from "./MenuSoundPreview";
import { ReplaceAppDialog } from "./ReplaceAppDialog";
import { ExportChoices, exportProduct, type Platform } from "./ExportChoices";
import { FirmwarePicker } from "./FirmwarePicker";
import appIcon from "../src-tauri/icons/icon.png";
import largeIcon from "../src-tauri/icons/icon-large.png";
import { MenuControlsEditor } from "./MenuControlsEditor";
import shaderCatalog from "../../integrations/shaders/catalog.json";
import {
  NOT_A_SHADER_FILE,
  SHADER_ACCEPT,
  SHADER_FORMATS,
  isShaderFile,
  shaderFileName,
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
import "./style.css";

const steps = ["Game", "Details", "Menu", "Export"];

type Selection = {
  path: string;
  name: string;
  size?: number;
  browserFile?: File;
};
// The initial settings of a dropped game: the declared builder defaults,
// except those we keep apart from the draft. We read the same declaration in
// the exporter, so a command-line export of the game alone gives this draft.
const {
  online: _online,
  theme: _theme,
  palette: _palette,
  menuSounds: _menuSounds,
  ...declaredDraft
} = declared;
// A project can list Options entries, which have no control here. With null
// we use the entries of the design.
const defaults = {
  title: "",
  system: "",
  description: "",
  menuEntries: null as string[] | null,
  ...declaredDraft,
};
type Draft = typeof defaults;

// We name three tracks. Six is more than a handful, so we show a count.
const NAMED_COMPANIONS = 5;

function alsoImporting(files: string[]): string | null {
  if (files.length < 2) return null;
  const extras = files.slice(1);
  if (extras.length > NAMED_COMPANIONS) {
    return `Also importing ${extras.length} files`;
  }
  const dot = files[0].lastIndexOf(".");
  const stem = dot > 0 ? files[0].slice(0, dot) : files[0];
  const named = extras.map((name) => {
    if (!stem || !name.startsWith(stem)) return name;
    const rest = name.slice(stem.length).trim();
    return rest || name;
  });
  return `Also importing: ${named.join(", ")}`;
}

function AlsoImporting({ files }: { files: string[] }) {
  const also = alsoImporting(files);
  if (!also) return null;
  return <p className="traveling-also">{also}</p>;
}

function StartupOptions({
  draft,
  update,
  startAtMenu,
}: {
  draft: Draft;
  update: <K extends keyof Draft>(key: K, value: Draft[K]) => void;
  startAtMenu: boolean;
}) {
  return (
    <>
      <Checkbox
        className="splash-choice"
        label="Startup logo"
        checked={draft.splash}
        onChange={(value) => update("splash", value)}
        help="Show a brief ROM-in-a-Box logo in the game window at startup."
      />
      <Checkbox
        label="Achievements"
        checked={draft.showMenu && draft.includeAchievements}
        disabled={!draft.showMenu}
        onChange={(value) => update("includeAchievements", value)}
        help={
          draft.showMenu
            ? "Let the player sign in to RetroAchievements and earn achievements."
            : "Requires game menu."
        }
      />
      <Checkbox
        label="Keep playing in the background"
        checked={draft.keepPlayingInBackground}
        onChange={(value) => update("keepPlayingInBackground", value)}
        help="Let the game keep running when its window is not in front. When off, it pauses until the player returns."
      />
      <Checkbox
        label="Autosave on quit"
        checked={draft.autosaveOnQuit}
        onChange={(value) => update("autosaveOnQuit", value)}
        help="Save the game when the player quits, and continue from there at the next launch."
      />
      {startAtMenu && (
        <Checkbox
          label="Show menu at startup"
          checked={draft.startAtMenu}
          onChange={(value) => update("startAtMenu", value)}
          help="Start at the menu before playing. The menu is also available during play."
        />
      )}
    </>
  );
}
/** The ROM-in-a-Box icon, for a game until the author chooses an icon. */
function IconArt() {
  return <img className="icon-art" src={largeIcon} alt="" />;
}
export function App() {
  const [step, setStep] = useState(0);
  const [supported, setSupported] = useState<Set<string>>(new Set());
  const [host, setHost] = useState<bridge.ExportTarget | null>(null);
  const [platform, setPlatform] = useState<Platform | null>(null);
  const exportTarget = platform === "both" ? host : (platform ?? host);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [info, setInfo] = useState<bridge.GameInfo | null>(null);
  const [traveling, setTraveling] = useState<string[]>([]);
  const [draft, setDraft] = useState<Draft>(defaults);
  const [icon, setIcon] = useState<bridge.Picture | null>(null);
  const [background, setBackground] = useState<bridge.Picture | null>(null);
  const [online, setOnline] = useState(declared.online);
  const [palette, setPalette] = useState(declared.palette);
  // The design the author picked, set through the selector and used in the
  // export.
  const [design, setDesign] = useState(declared.theme);
  const [menuSounds, setMenuSounds] = useState(declared.menuSounds);
  const [firmware, setFirmware] = useState<string[]>([]);
  const [bundledShaders, setBundledShaders] = useState<string[]>([]);
  const [customShaders, setCustomShaders] = useState<
    { name: string; path: string }[]
  >([]);
  const [shaderInitial, setShaderInitial] = useState<string | null>(null);
  const [firmwareAssessment, setFirmwareAssessment] =
    useState<bridge.FirmwareAssessment | null>(null);
  const [controls, setControls] = useState<Controls>(emptyControls);
  const [destination, setDestination] = useState("");
  const [busy, setBusy] = useState<"inspect" | "export" | "project" | null>(
    null,
  );
  const [progress, setProgress] = useState<bridge.ExportProgress | null>(null);
  const [replacing, setReplacing] = useState<bridge.AppExists | null>(null);
  const [coreNotice, setCoreNotice] = useState<CoreNotice | null>(null);
  // Progress events arrive on a separate channel. One that arrives after the
  // export has ended must not reopen the pop-up.
  const exporting = useRef(false);
  const [result, setResult] = useState<bridge.ExportResult | null>(null);
  const [error, setError] = useState("");
  const [savedProject, setSavedProject] = useState("");
  const [dragging, setDragging] = useState(false);
  const [preview, setPreview] = useState("/native-menu.png");
  const [previewBusy, setPreviewBusy] = useState(false);
  const [previewError, setPreviewError] = useState("");
  const gameInput = useRef<HTMLInputElement>(null);
  const shaderInput = useRef<HTMLInputElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
  const imageTarget = useRef<"icon" | "background">("icon");
  const heading = useRef<HTMLHeadingElement>(null);
  const generation = useRef(0);
  // Changing step clears the error, and we set a refusal found on entering
  // the details step in that same turn. Here we put the message back once
  // after the clear, so it stays and does not follow to a later step.
  const errorAfterStep = useRef<string | null>(null);
  const imageGeneration = useRef({ icon: 0, background: 0 });
  const nativeDropHandler = useRef<
    (paths: string[], pos: { x: number; y: number }) => void
  >(() => {});
  const imageURLs = useRef<string[]>([]);
  function holdURL(url: string) {
    imageURLs.current.push(url);
    return url;
  }
  function update<K extends keyof Draft>(key: K, value: Draft[K]) {
    if (key === "system" && value !== draft.system)
      setControls(emptyControls());
    setDraft((current) => ({ ...current, [key]: value }));
  }
  async function chooseSystem(system: string) {
    if (!selection || system === draft.system) return;
    const request = ++generation.current;
    // We inspect before we change the console. When we reject a console in
    // inspection, we keep the valid one and its Also importing line, so the
    // page and the export agree. A browser file has no path, but we still
    // check its extension.
    try {
      if (bridge.native && selection.path) {
        setBusy("inspect");
        await bridge.inspectGame(selection.path, online, system);
      } else if (selection.browserFile) {
        await inspectRom(selection.browserFile, undefined, system);
      }
    } catch (e) {
      if (request === generation.current) {
        setBusy(null);
        fail(e);
      }
      return;
    }
    if (request !== generation.current) return;
    update("system", system);
    setError("");
    if (!selection.path) {
      if (request === generation.current) setBusy(null);
      return;
    }
    // The same disc can have different companions on another console, so we
    // request the Also importing line again with the console now chosen.
    try {
      const traveled = await filesThatTravel(
        selection.path,
        selection.name,
        system,
      );
      if (request !== generation.current) return;
      setTraveling(traveled.files);
    } catch (e) {
      if (request !== generation.current) return;
      // We rejected the set in the exporter. A one-file line would describe a
      // different game from the one we copy in the export.
      setTraveling([]);
      fail(e);
    } finally {
      if (request === generation.current) setBusy(null);
    }
  }
  function fail(reason: unknown) {
    const message = reason instanceof Error ? reason.message : String(reason);
    errorAfterStep.current = message;
    setError(message);
  }
  function choose(value: Selection) {
    if (busy === "export") return;
    generation.current++;
    imageGeneration.current.icon++;
    imageGeneration.current.background++;
    setSelection(value);
    setInfo(null);
    setTraveling([]);
    setDraft(defaults);
    setControls(emptyControls());
    setFirmware([]);
    setIcon(null);
    setBackground(null);
    setError("");
    setResult(null);
    setSavedProject("");
    setBusy(null);
    setStep(0);
  }
  function browserChoose(files: FileList | null) {
    if (!files?.length) return;
    if (files.length !== 1) {
      setError("Choose one game at a time.");
      return;
    }
    const file = files[0] as File & { path?: string };
    choose({
      path: typeof file.path === "string" ? file.path : "",
      name: file.name,
      size: file.size,
      browserFile: file,
    });
  }
  async function filesThatTravel(
    filePath: string,
    fallbackName: string,
    system: string,
  ) {
    if (!filePath)
      return { files: fallbackName ? [fallbackName] : [], entry: "" };
    const listed = await bridge.travelingFiles(filePath, system);
    if (listed.files.length > 0)
      return { files: listed.files, entry: listed.entry || filePath };
    return { files: fallbackName ? [fallbackName] : [], entry: filePath };
  }
  async function chooseGame() {
    setError("");
    if (!bridge.native) {
      gameInput.current?.click();
      return;
    }
    try {
      const path = await bridge.pickFile("game");
      if (path) choose({ path, name: path.split(/[\\/]/).pop() || path });
    } catch (e) {
      fail(e);
    }
  }
  async function loadPicture(target: "icon" | "background", path: string) {
    const request = ++imageGeneration.current[target];
    const picture = await bridge.readImage(path);
    if (request !== imageGeneration.current[target]) {
      URL.revokeObjectURL(picture.url);
      return;
    }
    holdURL(picture.url);
    (target === "icon" ? setIcon : setBackground)(picture);
  }
  async function choosePicture(target: "icon" | "background") {
    setError("");
    imageTarget.current = target;
    if (!bridge.native) {
      imageInput.current?.click();
      return;
    }
    try {
      const path = await bridge.pickFile("image");
      if (path) await loadPicture(target, path);
    } catch (e) {
      fail(e);
    }
  }
  async function browserPicture(file?: File, target = imageTarget.current) {
    if (!file) return;
    if (file.size > 32 * 1024 * 1024) {
      setError("Choose an image smaller than 32 MB.");
      return;
    }
    const url = URL.createObjectURL(file);
    try {
      const image = new Image();
      image.src = url;
      await image.decode();
      holdURL(url);
      (target === "icon" ? setIcon : setBackground)({ path: "", url });
    } catch {
      URL.revokeObjectURL(url);
      setError("This file could not be opened as an image.");
    }
  }
  async function identify() {
    if (!selection || busy) return;
    if (info) {
      setStep(1);
      return;
    }
    const request = ++generation.current;
    setBusy("inspect");
    setError("");
    setStep(1);
    // We identify a cartridge before the next paint, so this screen is
    // normally gone before a screenshot. With `?hold-lookup` we keep it
    // visible, and nothing else sets that.
    if (new URLSearchParams(window.location.search).has("hold-lookup")) {
      await new Promise((resolve) => window.setTimeout(resolve, 800));
    }
    try {
      const data: bridge.GameInfo = bridge.native
        ? await bridge.inspectGame(selection.path, online)
        : {
            ...(await inspectRom(selection.browserFile!)),
            matched: false,
            warnings: [],
            source: "filename",
          };
      if (request !== generation.current) return;
      let traveled: Awaited<ReturnType<typeof filesThatTravel>> | null = null;
      try {
        traveled = await filesThatTravel(
          selection.path,
          selection.name,
          data.system,
        );
      } catch (e) {
        if (request !== generation.current) return;
        // We rejected the set in the exporter, so there is no one-file game.
        setTraveling([]);
        fail(e);
      }
      if (request !== generation.current) return;
      if (traveled) {
        setTraveling(traveled.files);
        const entryName = traveled.entry.split(/[\\/]/).pop();
        if (traveled.entry && entryName && entryName !== selection.name) {
          const entry = traveled.entry;
          setSelection((current) =>
            current && current.path === selection.path
              ? { ...current, path: entry, name: entryName }
              : current,
          );
        }
      }
      setInfo(data);
      setDraft({
        ...defaults,
        title: data.title,
        system: data.system,
        description: data.description || "",
      });
      if (data.iconPath && bridge.native)
        loadPicture("icon", data.iconPath).catch(() => {});
    } catch (e) {
      if (request === generation.current) fail(e);
    } finally {
      if (request === generation.current) setBusy(null);
    }
  }
  // Dropping a file moves straight to the next step, because the lookup
  // screen exists only on that step.
  useEffect(() => {
    if (!selection || info) return;
    void identify();
  }, [selection]);
  function goToStep(nextStep: number) {
    if (busy || nextStep === step) return;
    if (nextStep === 0) {
      setStep(0);
      return;
    }
    if (nextStep === 1 && selection) {
      void identify();
      return;
    }
    if (nextStep > 1 && info && validDetails) setStep(nextStep);
  }
  function exportRequest(): bridge.ExportRequest {
    return {
      rom: selection!.path,
      title: draft.title.trim(),
      system: draft.system,
      description: draft.description,
      icon: icon?.path || null,
      background: background?.path || null,
      showMenu: draft.showMenu,
      includeAchievements: draft.includeAchievements,
      splash: draft.splash,
      keepPlayingInBackground: draft.keepPlayingInBackground,
      autosaveOnQuit: draft.autosaveOnQuit,
      advancedEmulatorAccess: draft.advancedEmulatorAccess,
      intelMacs: draft.intelMacs,
      menuControls: draft.menuControls,
      menuEntries: draft.menuEntries,
      shaders: {
        bundled: bundledShaders,
        custom: customShaders,
        initial: shaderInitial,
      },
      startAtMenu: draft.showMenu && draft.startAtMenu,
      theme: design,
      palette,
      menuSounds,
      controls,
      firmware,
      outputDir: destination,
      target: exportTarget,
      bothPlatforms: platform === "both",
    };
  }
  async function saveProject() {
    if (!selection || busy || !bridge.native) return;
    setError("");
    try {
      const path = await bridge.pickProjectSave(draft.title);
      if (!path) return;
      setBusy("project");
      const saved = await bridge.saveProject(path, exportRequest());
      setSavedProject(saved.archivePath);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  }
  async function openProject() {
    if (busy || !bridge.native) return;
    setError("");
    try {
      const path = await bridge.pickProjectOpen();
      if (!path) return;
      setBusy("project");
      // In open_project we reject a design or palette that this build lacks.
      const loaded = await bridge.openProject(path);
      const settings = loaded.settings;
      generation.current++;
      imageGeneration.current.icon++;
      imageGeneration.current.background++;
      setSelection({
        path: settings.rom,
        name: settings.rom.split(/[\\/]/).pop() || settings.title,
      });
      setDraft({
        title: settings.title,
        system: settings.system,
        description: settings.description || "",
        showMenu: settings.showMenu,
        includeAchievements: settings.includeAchievements,
        splash: settings.splash ?? false,
        keepPlayingInBackground: settings.keepPlayingInBackground ?? false,
        autosaveOnQuit: settings.autosaveOnQuit ?? false,
        advancedEmulatorAccess: settings.advancedEmulatorAccess ?? false,
        intelMacs: settings.intelMacs,
        menuControls: settings.menuControls,
        menuEntries: settings.menuEntries ?? null,
        startAtMenu: settings.startAtMenu,
      });
      setPalette(settings.palette);
      setPlatform(settings.bothPlatforms ? "both" : settings.target);
      // A project contains the design it was saved with, and in the check
      // above we already reject one that this build does not have. We restore
      // it on reopen to keep the choice of the author.
      setDesign(settings.theme);
      setMenuSounds(settings.menuSounds || "off");
      setControls(settings.controls || emptyControls());
      setFirmware(settings.firmware || []);
      setBundledShaders(settings.shaders?.bundled ?? []);
      setCustomShaders(settings.shaders?.custom ?? []);
      setShaderInitial(settings.shaders?.initial ?? null);
      const filename = settings.rom.split(/[\\/]/).pop() || settings.title;
      try {
        const traveled = await filesThatTravel(
          settings.rom,
          filename,
          settings.system,
        );
        setTraveling(traveled.files);
      } catch (e) {
        setTraveling([]);
        fail(e);
      }
      setInfo({
        title: settings.title,
        system: settings.system,
        filename,
        size: 0,
        source: "project",
        matched: false,
        warnings: [],
      });
      setIcon(null);
      setBackground(null);
      setResult(null);
      setSavedProject(path);
      if (settings.icon) await loadPicture("icon", settings.icon);
      if (settings.background)
        await loadPicture("background", settings.background);
      setStep(1);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  }
  async function packageGame(replace = false) {
    if (!selection || busy || !bridge.native) return;
    setError("");
    setBusy("export");
    setCoreNotice(null);
    setProgress({
      stage: "preparing",
      fraction: 0,
      message: "Preparing your game…",
    });
    exporting.current = true;
    try {
      const value = await bridge.exportGame({ ...exportRequest(), replace });
      setResult(value);
      setCoreNotice(afterExport());
    } catch (e) {
      // We did nothing. The author chooses, and on Replace we export again.
      if (e instanceof bridge.AppExists) {
        setReplacing(e);
        return;
      }
      const notice = afterExport(e);
      setCoreNotice(notice);
      if (!notice) fail(e);
    } finally {
      exporting.current = false;
      setBusy(null);
    }
  }
  function togglePreset(id: string) {
    const on = bundledShaders.includes(id);
    setBundledShaders(
      on
        ? bundledShaders.filter((item) => item !== id)
        : [...bundledShaders, id],
    );
    if (on) setShaderInitial((current) => (current === id ? null : current));
  }
  function removeCustomShader(shader: { name: string; path: string }) {
    setCustomShaders((current) =>
      current.filter((item) => item.path !== shader.path),
    );
    setShaderInitial((current) => (current === shader.name ? null : current));
  }
  function addCustomShader(filePath: string) {
    if (!isShaderFile(filePath)) {
      setError(NOT_A_SHADER_FILE);
      return;
    }
    const name = shaderFileName(filePath);
    setCustomShaders((current) => {
      if (current.some((item) => item.path === filePath)) return current;
      return [...current, { name, path: filePath }];
    });
  }
  async function chooseShaderFile() {
    setError("");
    if (!bridge.native) {
      shaderInput.current?.click();
      return;
    }
    try {
      const path = await bridge.pickShader();
      if (path) addCustomShader(path);
    } catch (e) {
      fail(e);
    }
  }
  nativeDropHandler.current = (paths, pos) => {
    if (busy) return;
    const target = document
      .elementFromPoint(pos.x, pos.y)
      ?.closest("[data-drop]")
      ?.getAttribute("data-drop");
    if (paths.length !== 1) {
      setError("Drop one file at a time.");
      return;
    }
    if (target === "icon" || target === "background")
      loadPicture(target, paths[0]).catch(fail);
    else if (target === "shader") addCustomShader(paths[0]);
    else if (step === 0)
      choose({
        path: paths[0],
        name: paths[0].split(/[\\/]/).pop() || paths[0],
      });
  };
  useEffect(() => {
    // The browser walkthrough has no kit and no backend, so we supply the
    // answers of the desktop commands here: which consoles already have a
    // core on disk, and which platform we export for.
    if (bridge.native) return;
    const walkthrough = window as Window & {
      __ROMINABOX_PREPARED__?: string[];
      __ROMINABOX_EXPORT_TARGET__?: bridge.ExportTarget;
    };
    if (walkthrough.__ROMINABOX_PREPARED__)
      setSupported(new Set(walkthrough.__ROMINABOX_PREPARED__));
    if (walkthrough.__ROMINABOX_EXPORT_TARGET__)
      setHost(walkthrough.__ROMINABOX_EXPORT_TARGET__);
  }, []);
  useEffect(() => {
    if (!bridge.native) return;
    let disposed = false;
    const cleanups: (() => void)[] = [];
    const save = (fn: () => void) => {
      if (disposed) fn();
      else cleanups.push(fn);
    };
    bridge
      .onNativeDrop(
        (paths, pos) => nativeDropHandler.current(paths, pos),
        setDragging,
      )
      .then(save)
      .catch(fail);
    bridge
      .onExportProgress((value) => {
        if (!exporting.current) return;
        // We show it in the pop-up and leave the progress line as it was.
        if (value.cores)
          setCoreNotice((current) => afterProgress(current, value));
        else setProgress(value);
      })
      .then(save)
      .catch(fail);
    bridge.defaultDestination().then(setDestination).catch(fail);
    bridge.exportTarget().then(setHost).catch(fail);
    // Which cores are already on disk.
    bridge
      .availableSystems()
      .then((ids) => {
        if (!disposed) setSupported(new Set(ids));
      })
      .catch(fail);
    return () => {
      disposed = true;
      cleanups.forEach((fn) => fn());
    };
  }, []);
  useEffect(() => {
    setSavedProject("");
  }, [draft, icon, background, palette, menuSounds, controls, firmware]);
  useEffect(
    () => () => {
      imageURLs.current.forEach((url) => URL.revokeObjectURL(url));
    },
    [],
  );
  useEffect(() => {
    heading.current?.focus();
    setError("");
  }, [step]);
  useEffect(() => {
    if (!errorAfterStep.current) return;
    const message = errorAfterStep.current;
    errorAfterStep.current = null;
    setError(message);
  });
  // A disclosure on the Menu step opens below the fold, because the preview
  // above it is 445 points tall and the content area of the step scrolls.
  // When someone opens one, we scroll it into view.
  //
  // We use one listener for all five disclosures, because `toggle` does not
  // bubble but can be captured.
  //
  // We scroll only when the opened content does not fit, and then put the
  // summary at the top so the content has the whole area under it. If we
  // scrolled as little as possible, a tall section would show only its first
  // few points.
  useEffect(() => {
    const opened = (event: Event) => {
      const element = event.target;
      if (!(element instanceof HTMLDetailsElement) || !element.open) return;
      const area = element.closest(".screen");
      const bottom = area
        ? area.getBoundingClientRect().bottom
        : window.innerHeight;
      if (element.getBoundingClientRect().bottom <= bottom) return;
      // After the next frame. `toggle` fires when the attribute changes, and
      // at that moment the scrolling area has not grown yet, so scrolling here
      // moves nothing.
      requestAnimationFrame(() => element.scrollIntoView({ block: "start" }));
    };
    document.addEventListener("toggle", opened, true);
    return () => document.removeEventListener("toggle", opened, true);
  }, []);
  // We draw the controller scene and the menu frame in the colours of the
  // palette the author picked, so the editor matches the exported game.
  useEffect(() => {
    const chosen = designs.palettes.find((entry) => entry.id === palette);
    if (!chosen) return;
    const root = document.documentElement;
    const applied: string[] = [];
    const set = (name: string, value: string) => {
      root.style.setProperty(`--palette-${name}`, value);
      applied.push(`--palette-${name}`);
    };
    for (const [name, value] of Object.entries(chosen)) {
      if (typeof value === "string" && value.startsWith("#")) set(name, value);
    }
    for (const [name, value] of Object.entries(chosen.tokens ?? {})) {
      if (typeof value === "string") set(name, value);
    }
    return () => applied.forEach((name) => root.style.removeProperty(name));
  }, [palette]);

  useEffect(() => {
    if (step !== 2 || !draft.showMenu) return;
    if (!bridge.native) {
      setPreview(`/native-menu-${palette}.png`);
      return;
    }
    let cancelled = false;
    setPreviewBusy(true);
    setPreviewError("");
    bridge
      .menuPreview(background?.path || null, palette, design)
      .then((url) => {
        if (cancelled) URL.revokeObjectURL(url);
        else {
          holdURL(url);
          setPreview(url);
        }
      })
      .catch(() => {
        if (!cancelled) setPreviewError("The preview could not be rendered.");
      })
      .finally(() => {
        if (!cancelled) setPreviewBusy(false);
      });
    return () => {
      cancelled = true;
    };
    // The design too. We draw the picture beside the chooser from the chosen
    // design, so it matches what we export.
  }, [step, draft.showMenu, background?.path, palette, design]);
  function imageDrop(e: React.DragEvent, target: "icon" | "background") {
    e.preventDefault();
    e.stopPropagation();
    browserPicture(e.dataTransfer.files[0], target);
  }
  const systemDefinition = SYSTEMS.find(
    (s) =>
      s.id.toLowerCase() === draft.system.toLowerCase() ||
      s.aliases.some(
        (alias) => alias.toLowerCase() === draft.system.toLowerCase(),
      ),
  );
  const systemName = systemDefinition?.name || "Choose a console";

  const requirements = systemDefinition?.firmware || [];
  const asksFirmware = requirements.length > 0;
  const firmwareBlocked =
    asksFirmware && firmwareAssessment?.canContinue !== true;
  // We describe an optional BIOS in the help of the button, not on the page.
  const { Unmatched, Optional } = bridge.FirmwareNoticeKind;
  const firmwareNotices = (firmwareAssessment?.notices ?? []).filter(
    (notice) => notice.kind !== Unmatched && notice.kind !== Optional,
  );
  const optionalHelp = firmwareAssessment?.notices.find(
    (notice) => notice.kind === Optional,
  )?.text;
  const firmwareStops = (firmwareAssessment?.notices ?? []).some(
    (notice) =>
      notice.kind === bridge.FirmwareNoticeKind.Required ||
      notice.kind === bridge.FirmwareNoticeKind.Duplicate,
  );
  useEffect(() => {
    if (!systemDefinition || requirements.length === 0) {
      setFirmwareAssessment(null);
      return;
    }
    let cancelled = false;
    const systemId = systemDefinition.id;
    setFirmwareAssessment(null);
    bridge
      .assessFirmware(systemId, firmware)
      .then((assessment) => {
        if (!cancelled) setFirmwareAssessment(assessment);
      })
      .catch((reason: unknown) => {
        if (!cancelled && bridge.native) fail(reason);
      });
    return () => {
      cancelled = true;
    };
  }, [systemDefinition, firmware]);
  const validDetails = !!draft.title.trim() && !!draft.system;
  const stepAvailable = (index: number) =>
    index === 0 ||
    (index === 1 ? !!selection : !!info && validDetails && !firmwareBlocked);
  const canNext =
    step === 0
      ? !!selection
      : step === 1
        ? !!info && validDetails && !firmwareBlocked
        : !firmwareBlocked;
  const progressPercent = Math.round(
    Math.max(0, Math.min(1, progress?.fraction || 0)) * 100,
  );
  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="wordmark">
          <img className="brand-mark" src={appIcon} alt="" />
          <span>ROM-in-a-Box</span>
        </div>
        <span className="edition">
          {bridge.native ? "GAME APP BUILDER" : "BROWSER PREVIEW"}
        </span>
      </header>
      <main>
        <nav className="steps" aria-label="Progress">
          {steps.map((name, i) => (
            <React.Fragment key={name}>
              {i > 0 && <span className="step-line" />}
              <button
                type="button"
                aria-label={name}
                className={`step ${step === i ? "current" : ""} ${step > i ? "done" : ""}`}
                aria-current={step === i ? "step" : undefined}
                disabled={!!busy || !stepAvailable(i)}
                onClick={() => goToStep(i)}
              >
                <span className="step-number">
                  {step > i ? (
                    <Check size={14} />
                  ) : (
                    String(i + 1).padStart(2, "0")
                  )}
                </span>
                {name}
              </button>
            </React.Fragment>
          ))}
        </nav>
        <section key={step} className={`screen screen-${step}`}>
          {step === 0 ? (
            <>
              <h1 ref={heading} tabIndex={-1}>
                Choose a game
              </h1>
              <div
                className={`drop-zone ${dragging ? "dragging" : ""} ${selection ? "has-file" : ""}`}
                data-drop="game"
                onDragOver={(e) => {
                  e.preventDefault();
                  setDragging(true);
                }}
                onDragLeave={() => setDragging(false)}
                onDrop={(e) => {
                  e.preventDefault();
                  setDragging(false);
                  browserChoose(e.dataTransfer.files);
                }}
              >
                <IconArt />
                <h2>{selection ? selection.name : "Drop your game here"}</h2>
                {selection?.size !== undefined && (
                  <span className="file-size">
                    {formatBytes(selection.size)}
                  </span>
                )}
                <button className="secondary" onClick={chooseGame}>
                  {selection ? "Change file" : "Choose a file"}
                  <Plus size={17} />
                </button>
              </div>
              <div className="opening-tools">
                <details className="advanced lookup-settings">
                  <summary>
                    <ChevronRight size={16} />
                    Advanced
                  </summary>
                  <Checkbox
                    label="Look up game details"
                    checked={online}
                    onChange={(value) => {
                      setOnline(value);
                      setInfo(null);
                    }}
                    help="Use established game catalogs and artwork. Only the matched title is sent to look up the cover. The ROM stays on your computer."
                  />
                </details>
                {bridge.native && (
                  <div className="project-action">
                    <button
                      className="text-button"
                      disabled={!!busy}
                      onClick={openProject}
                    >
                      {busy === "project" ? (
                        <LoaderCircle size={16} className="spin" />
                      ) : (
                        <FolderOpen size={16} />
                      )}{" "}
                      Open project
                    </button>
                    <Help label="About opening projects">
                      Open a saved build, including its game, images and
                      settings, to make changes or export again.
                    </Help>
                  </div>
                )}
              </div>
            </>
          ) : step === 1 ? (
            <>
              <div className="heading-row">
                <h1 ref={heading} tabIndex={-1}>
                  Game details
                </h1>
                {info && validDetails && !firmwareBlocked && (
                  <span className="ready">
                    <Check size={17} />
                    Ready to go
                  </span>
                )}
                {/* We disable Next while a console requires a BIOS that the
                    author has not added, and while the check for a BIOS is
                    still running. We show different text for the two states,
                    because a console whose BIOS is optional must not look as
                    if it were missing one. */}
                {info && validDetails && firmwareBlocked && (
                  <span className="waiting" role="status">
                    {firmwareAssessment === null ? (
                      <>
                        <LoaderCircle size={17} className="spin" />
                        Checking what {systemName} needs
                      </>
                    ) : (
                      <>
                        <TriangleAlert size={17} />
                        {systemName} needs a BIOS file
                      </>
                    )}
                  </span>
                )}
              </div>
              {busy === "inspect" ? (
                <div className="lookup" role="status">
                  <LoaderCircle size={38} className="spin" />
                  <h2>
                    {online ? "Finding your game…" : "Reading your game…"}
                  </h2>
                  <progress aria-label="Game identification" />
                  <button
                    className="text-button"
                    onClick={() => {
                      generation.current++;
                      setBusy(null);
                      setStep(0);
                    }}
                  >
                    Cancel
                  </button>
                </div>
              ) : info ? (
                <>
                  <div className="details-layout">
                    <div>
                      <button
                        className="icon-picker"
                        data-drop="icon"
                        onClick={() => choosePicture("icon")}
                        onDragOver={(e) => e.preventDefault()}
                        onDrop={(e) => imageDrop(e, "icon")}
                        aria-label="Choose or drop an app icon"
                      >
                        {icon ? (
                          <img src={icon.url} alt="App icon" />
                        ) : (
                          <IconArt />
                        )}
                        <span>
                          <FileImage size={16} />
                          {icon ? "Change icon" : "Add an icon"}
                        </span>
                      </button>
                      <div className="icon-meta">
                        <Help>
                          Drop an image here. The builder creates the required
                          icon sizes and preserves its proportions.
                        </Help>
                        {icon && (
                          <button
                            className="text-button"
                            onClick={() => {
                              imageGeneration.current.icon++;
                              setIcon(null);
                            }}
                          >
                            Remove
                          </button>
                        )}
                      </div>
                    </div>
                    <div className="fields">
                      {traveling.length > 0 && (
                        <div className="traveling" data-traveling>
                          <p>{traveling[0]}</p>
                          <AlsoImporting files={traveling} />
                        </div>
                      )}
                      <label>
                        Game name
                        <input
                          autoComplete="off"
                          maxLength={100}
                          value={draft.title}
                          onChange={(e) => update("title", e.target.value)}
                        />
                      </label>
                      <label>
                        Console
                        <select
                          value={systemDefinition?.id || draft.system}
                          onChange={(e) => chooseSystem(e.target.value)}
                        >
                          <option value="" disabled>
                            Choose a console
                          </option>
                          {SYSTEMS.map((s) => (
                            <option
                              key={s.id}
                              value={s.id}
                              disabled={!canExport(supported, s.id)}
                            >
                              {s.name}
                              {whyNot(supported, s.id)
                                ? ` — ${whyNot(supported, s.id)}`
                                : ""}
                            </option>
                          ))}
                        </select>
                      </label>
                      <div className="match-note">
                        {info.matched && <Check size={15} />}
                        <span>
                          {info.matched
                            ? "Catalog match"
                            : info.source === "project"
                              ? "Saved project"
                              : !draft.system
                                ? "Choose a console"
                                : "Details from your file"}
                        </span>
                        <Help>
                          {info.source === "project"
                            ? "Your saved configuration, including its game and artwork."
                            : info.matched
                              ? `Matched using ${info.source}${info.catalogName ? `: ${info.catalogName}` : "."}`
                              : "No exact catalog match was available. You can edit the name and choose the console."}
                        </Help>
                      </div>
                    </div>
                  </div>
                  {asksFirmware && (
                    <div
                      className={
                        firmwareStops
                          ? "firmware-required"
                          : "firmware-optional"
                      }
                    >
                      {firmwareNotices.map((notice) => (
                        <p
                          className="firmware-status"
                          key={notice.kind + notice.text}
                          role={
                            notice.kind === bridge.FirmwareNoticeKind.Required
                              ? "alert"
                              : "status"
                          }
                        >
                          {notice.text}
                        </p>
                      ))}
                      <FirmwarePicker
                        files={firmware}
                        onChange={setFirmware}
                        blocked={firmwareBlocked}
                        assessment={firmwareAssessment}
                        optionalHelp={optionalHelp}
                        onError={fail}
                      />
                    </div>
                  )}
                  <details className="advanced">
                    <summary>
                      <ChevronRight size={16} />
                      More details
                    </summary>
                    <label>
                      Description
                      <textarea
                        rows={3}
                        maxLength={2000}
                        value={draft.description}
                        onChange={(e) => update("description", e.target.value)}
                      />
                    </label>
                    {info.warnings?.map((w) => (
                      <p className="note" key={w}>
                        {w}
                      </p>
                    ))}
                    <p className="note filename">
                      {info.filename}
                      {info.size > 0 ? ` · ${formatBytes(info.size)}` : ""}
                    </p>
                  </details>
                </>
              ) : (
                <div className="lookup">
                  <p>We couldn’t read this game.</p>
                  <button
                    className="secondary"
                    onClick={() => {
                      setStep(0);
                    }}
                  >
                    Choose another file
                  </button>
                </div>
              )}
            </>
          ) : step === 2 ? (
            <>
              <div className="heading-row">
                <h1 ref={heading} tabIndex={-1}>
                  Choose a menu
                </h1>
                <Checkbox
                  label="Include game menu"
                  checked={draft.showMenu}
                  onChange={(value) => update("showMenu", value)}
                  help="Open the menu during gameplay to save, load, continue or quit."
                />
              </div>
              {draft.showMenu ? (
                <div className="menu-workspace">
                  <div className="menu-preview">
                    <div className="preview-label">
                      <label className="design-select">
                        Menu design{" "}
                        <select
                          aria-label="Menu design"
                          value={design}
                          onChange={(e) => setDesign(e.target.value)}
                        >
                          {designs.designs.map((design) => (
                            <option value={design.id} key={design.id}>
                              {design.name}
                            </option>
                          ))}
                        </select>
                      </label>
                      <div
                        className="palette-picker"
                        role="group"
                        aria-label="Colour palette"
                      >
                        {designs.palettes.map((p) => (
                          <button
                            key={p.id}
                            type="button"
                            aria-pressed={palette === p.id}
                            className={palette === p.id ? "active" : ""}
                            onClick={() => setPalette(p.id)}
                          >
                            <span
                              className="palette-swatch"
                              style={{
                                background: p.screen,
                                borderColor: p.surface,
                              }}
                            />
                            <span>{p.name}</span>
                          </button>
                        ))}
                      </div>
                      <span
                        className="preview-status"
                        role="status"
                        aria-label={
                          previewBusy ? "Updating preview" : undefined
                        }
                      >
                        {previewBusy && (
                          <LoaderCircle size={16} className="spin" />
                        )}
                      </span>
                    </div>
                    <div className="menu-frame">
                      <img
                        src={preview}
                        alt="Game menu with six save slots, Continue, Save, Load and Quit"
                      />
                    </div>
                    {previewError && <p className="error">{previewError}</p>}
                  </div>
                  <div className="menu-settings">
                    <StartupOptions draft={draft} update={update} startAtMenu />
                    <details className="advanced">
                      <summary>
                        <ChevronRight size={16} />
                        Customize
                      </summary>
                      <div className="sound-choice">
                        <label htmlFor="menu-sounds">Menu sounds</label>
                        <select
                          id="menu-sounds"
                          value={menuSounds}
                          onChange={(e) => setMenuSounds(e.target.value)}
                        >
                          {designs.soundPacks.map((pack) => (
                            <option value={pack.id} key={pack.id}>
                              {pack.name}
                            </option>
                          ))}
                        </select>
                        <MenuSoundPreview pack={menuSounds} />
                      </div>

                      <div className="customize-row">
                        <div
                          className="background-picker"
                          data-drop="background"
                          onDragOver={(e) => e.preventDefault()}
                          onDrop={(e) => imageDrop(e, "background")}
                        >
                          <button
                            className="secondary"
                            onClick={() => choosePicture("background")}
                          >
                            <FileImage size={17} />
                            {background
                              ? "Change background"
                              : "Add background"}
                          </button>
                          {background && (
                            <button
                              className="icon-button"
                              aria-label="Remove background"
                              onClick={() => {
                                imageGeneration.current.background++;
                                setBackground(null);
                              }}
                            >
                              <X size={18} />
                            </button>
                          )}
                        </div>
                      </div>
                    </details>
                  </div>
                </div>
              ) : (
                <div className="menu-off">
                  <IconArt />
                  <p>No in-game menu.</p>
                </div>
              )}
              {!draft.showMenu && (
                <div className="play-options">
                  <StartupOptions
                    draft={draft}
                    update={update}
                    startAtMenu={false}
                  />
                </div>
              )}

              <details className="advanced author-controls">
                <summary>
                  <ChevronRight size={16} />
                  Controls
                </summary>
                <ControlsEditor
                  system={draft.system}
                  value={controls}
                  onChange={setControls}
                />
                {draft.showMenu && (
                  <MenuControlsEditor
                    value={draft.menuControls}
                    busy={!!busy}
                    onChange={(value) => update("menuControls", value)}
                  />
                )}
              </details>
              <details className="advanced picture-filters">
                <summary>
                  <ChevronRight size={16} />
                  {bundledShaders.length + customShaders.length === 0
                    ? "Picture filters (shaders) · none selected"
                    : `Picture filters (shaders) · ${bundledShaders.length + customShaders.length} selected`}
                </summary>
                <div className="shader-choices">
                  <p className="shader-lede">
                    Pick the shaders players can choose in the game. Click one
                    to select it.
                  </p>
                  <div className="shader-grid">
                    {shaderCatalog.presets.map((preset) => {
                      const chosen = bundledShaders.includes(preset.id);
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
                    {customShaders.map((shader) => (
                      <button
                        key={shader.path}
                        type="button"
                        className="shader-card chosen"
                        aria-pressed={true}
                        onClick={() => removeCustomShader(shader)}
                      >
                        <span className="shader-name">{shader.name}</span>
                      </button>
                    ))}
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
                        addCustomShader(dropped.path || file.name);
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
                    ref={shaderInput}
                    type="file"
                    hidden
                    accept={SHADER_ACCEPT}
                    data-shader
                    onChange={(event) => {
                      const file = event.target.files?.[0];
                      if (file) addCustomShader(file.name);
                      event.target.value = "";
                    }}
                  />
                  {bundledShaders.length + customShaders.length > 0 && (
                    <label className="shader-initial">
                      Starts on
                      <select
                        aria-label="Starts on"
                        value={shaderInitial ?? ""}
                        onChange={(event) =>
                          setShaderInitial(event.target.value || null)
                        }
                      >
                        <option value="">Unfiltered</option>
                        {shaderCatalog.presets
                          .filter((preset) =>
                            bundledShaders.includes(preset.id),
                          )
                          .map((preset) => (
                            <option key={preset.id} value={preset.id}>
                              {preset.name}
                            </option>
                          ))}
                        {customShaders.map((shader) => (
                          <option key={shader.path} value={shader.name}>
                            {shader.name}
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                </div>
              </details>
              <details className="advanced">
                <summary>
                  <ChevronRight size={16} />
                  Advanced
                </summary>
                <Checkbox
                  label="Advanced emulator access"
                  checked={draft.advancedEmulatorAccess}
                  onChange={(value) => update("advancedEmulatorAccess", value)}
                  help="Restore RetroArch's native menus. Ordinary exports keep About, Hide, Quit and standard window actions."
                />
              </details>
            </>
          ) : (
            <>
              <h1 ref={heading} tabIndex={-1}>
                {result
                  ? "App created"
                  : busy === "export"
                    ? "Building your game…"
                    : "Export your game"}
              </h1>
              <div className="export-summary">
                <div className="summary-icon">
                  {icon ? <img src={icon.url} alt="" /> : <IconArt />}
                </div>
                <div>
                  <h2>{draft.title}</h2>
                  <p>{systemName}</p>
                  <span className="export-format">
                    {exportProduct(platform ?? host)}
                  </span>
                </div>
                {result && <Check className="complete-mark" size={38} />}
              </div>
              {busy === "export" ? (
                <div className="export-progress" role="status">
                  <div>
                    <span>{progress?.message}</span>
                    <b>{progressPercent}%</b>
                  </div>
                  <progress
                    max={1}
                    value={progress?.fraction || 0}
                    aria-label="Packaging progress"
                  />
                  <button
                    className="text-button"
                    onClick={() => bridge.cancelExport().catch(fail)}
                  >
                    Cancel
                  </button>
                </div>
              ) : result ? (
                <div className="result">
                  <dl>
                    <div>
                      <dt>App</dt>
                      <dd>{formatBytes(result.installedBytes)}</dd>
                    </div>
                  </dl>
                  <button
                    className="secondary"
                    onClick={() => bridge.reveal(result.appPath).catch(fail)}
                  >
                    <FolderOpen size={18} />
                    Show app
                  </button>
                </div>
              ) : (
                <>
                  <ExportChoices
                    host={host}
                    target={platform ?? host}
                    onTarget={setPlatform}
                    destination={destination}
                    onDestination={setDestination}
                    intelMacs={draft.intelMacs}
                    onIntelMacs={(value) => update("intelMacs", value)}
                    fail={fail}
                  />
                  {!bridge.native && (
                    <p className="note">
                      Export is available in the desktop app.
                    </p>
                  )}
                </>
              )}
            </>
          )}
        </section>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <footer className="navigation">
          <div>
            {step > 0 && !result && (
              <button
                className="back"
                disabled={!!busy}
                onClick={() => setStep(step - 1)}
              >
                <ArrowLeft size={18} />
                Back
              </button>
            )}
            {step === 0 && <span className="edition"></span>}
            {step === 3 && bridge.native && (
              <div className="project-action">
                <button
                  className="secondary"
                  disabled={!!busy}
                  onClick={saveProject}
                >
                  <Save size={17} />
                  {busy === "project" ? "Saving…" : "Save project"}
                </button>
                <Help label="About saving projects">
                  Save the game, images and settings in one project file that
                  you can reopen and edit.
                </Help>
              </div>
            )}
            {step === 3 && savedProject && (
              <button
                className="text-button"
                onClick={() => bridge.reveal(savedProject).catch(fail)}
              >
                <Check size={15} />
                Saved
              </button>
            )}
          </div>
          {step < 3 ? (
            <button
              className="primary"
              disabled={!canNext || !!busy}
              onClick={() => (step === 0 ? identify() : setStep(step + 1))}
            >
              Next
              <ArrowRight size={19} />
            </button>
          ) : result ? (
            <button
              className="primary"
              onClick={() => {
                generation.current++;
                setStep(0);
                setSelection(null);
                setInfo(null);
                setDraft(defaults);
                setIcon(null);
                setBackground(null);
                setResult(null);
                setError("");
              }}
            >
              Package another
              <Plus size={18} />
            </button>
          ) : (
            <button
              className="primary"
              disabled={
                !!busy || !bridge.native || !destination || firmwareBlocked
              }
              onClick={() => packageGame()}
            >
              {busy === "export" ? (
                <LoaderCircle className="spin" size={18} />
              ) : (
                "Create app"
              )}
              {!busy && <ArrowRight size={19} />}
            </button>
          )}
        </footer>
      </main>
      {replacing && (
        <ReplaceAppDialog
          existing={replacing}
          onCancel={() => setReplacing(null)}
          onReplace={() => {
            setReplacing(null);
            packageGame(true);
          }}
        />
      )}
      {coreNotice && (
        <CoreFetchNotice
          notice={coreNotice}
          onBack={() => setCoreNotice(null)}
          onRetry={() => {
            setCoreNotice(null);
            packageGame();
          }}
        />
      )}
      <input
        ref={gameInput}
        type="file"
        hidden
        onChange={(e) => {
          browserChoose(e.target.files);
          e.target.value = "";
        }}
      />
      <input
        ref={imageInput}
        type="file"
        hidden
        accept="image/*"
        onChange={(e) => {
          browserPicture(e.target.files?.[0]);
          e.target.value = "";
        }}
      />
    </div>
  );
}
