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
  X,
} from "lucide-react";
import { SYSTEMS, formatBytes, inspectRom } from "./inspection";
import * as bridge from "./bridge";
import designs from "../designs.json";
import { ControlsEditor, emptyControls, type Controls } from "./controls";
import { Help, Checkbox } from "./Help";
import { MenuSoundPreview } from "./MenuSoundPreview";
import shaderCatalog from "../../integrations/shaders/catalog.json";
import "./style.css";

const steps = ["Game", "Details", "Menu", "Export"];

type Selection = {
  path: string;
  name: string;
  size?: number;
  browserFile?: File;
};
type Draft = {
  title: string;
  system: string;
  description: string;
  showMenu: boolean;
  startAtMenu: boolean;
  splash: boolean;
  advancedEmulatorAccess: boolean;
};
const defaults: Draft = {
  title: "",
  system: "",
  description: "",
  showMenu: true,
  startAtMenu: false,
  splash: true,
  advancedEmulatorAccess: false,
};
function Cartridge() {
  return (
    <svg className="cartridge" viewBox="0 0 120 136" aria-hidden="true">
      <path d="M18 4h84l14 16v108H4V20z" fill="currentColor" />
      <path d="M23 13h72v37H23z" fill="#fff6c9" />
      <path d="M28 18h14v26H28zm24 0h14v26H52zm24 0h14v26H76" fill="#e45c35" />
      <path d="M20 62h80v43H20z" fill="#fff6c9" />
      <path d="M29 72h62v5H29zm0 12h40v5H29z" fill="currentColor" />
      {[20, 34, 48, 62, 76, 90].map((x) => (
        <path key={x} d={`M${x} 115h5v13h-5z`} fill="#fff6c9" />
      ))}
    </svg>
  );
}
export function App() {
  const [step, setStep] = useState(0);
  const [supported, setSupported] = useState<Set<string>>(new Set());
  const [selection, setSelection] = useState<Selection | null>(null);
  const [info, setInfo] = useState<bridge.GameInfo | null>(null);
  const [draft, setDraft] = useState<Draft>(defaults);
  const [icon, setIcon] = useState<bridge.Picture | null>(null);
  const [background, setBackground] = useState<bridge.Picture | null>(null);
  const [online, setOnline] = useState(true);
  const [palette, setPalette] = useState("blue");
  const [menuSounds, setMenuSounds] = useState("off");
  const [firmware, setFirmware] = useState<string[]>([]);
  const [bundledShaders, setBundledShaders] = useState<string[]>([]);
  const [customShaders, setCustomShaders] = useState<
    { name: string; path: string }[]
  >([]);
  const [shaderInitial, setShaderInitial] = useState<string | null>(null);
  const [controls, setControls] = useState<Controls>(emptyControls);
  const [destination, setDestination] = useState("");
  const [busy, setBusy] = useState<"inspect" | "export" | "project" | null>(
    null,
  );
  const [progress, setProgress] = useState<bridge.ExportProgress | null>(null);
  const [result, setResult] = useState<bridge.ExportResult | null>(null);
  const [error, setError] = useState("");
  const [savedProject, setSavedProject] = useState("");
  const [dragging, setDragging] = useState(false);
  const [preview, setPreview] = useState("/native-menu.png");
  const [previewBusy, setPreviewBusy] = useState(false);
  const [previewError, setPreviewError] = useState("");
  const gameInput = useRef<HTMLInputElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
  const imageTarget = useRef<"icon" | "background">("icon");
  const heading = useRef<HTMLHeadingElement>(null);
  const generation = useRef(0);
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
    update("system", system);
    if (!bridge.native || !selection?.path || system === draft.system) return;
    const request = ++generation.current;
    setBusy("inspect");
    try {
      const data = await bridge.inspectGame(selection.path, online, system);
      if (request !== generation.current) return;
      const oldTitle = info?.title;
      setInfo(data);
      setDraft((current) => ({
        ...current,
        title: current.title === oldTitle ? data.title : current.title,
      }));
      if (!icon && data.iconPath) await loadPicture("icon", data.iconPath);
    } catch (e) {
      if (request === generation.current) fail(e);
    } finally {
      if (request === generation.current) setBusy(null);
    }
  }
  function fail(reason: unknown) {
    setError(reason instanceof Error ? reason.message : String(reason));
  }
  function choose(value: Selection) {
    if (busy === "export") return;
    generation.current++;
    imageGeneration.current.icon++;
    imageGeneration.current.background++;
    setSelection(value);
    setInfo(null);
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
    choose({
      path: "",
      name: files[0].name,
      size: files[0].size,
      browserFile: files[0],
    });
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
      splash: draft.splash,
      advancedEmulatorAccess: draft.advancedEmulatorAccess,
      shaders: {
        bundled: bundledShaders,
        custom: customShaders,
        initial: shaderInitial,
      },
      startAtMenu: draft.showMenu && draft.startAtMenu,
      theme: "native",
      palette,
      menuSounds,
      controls,
      firmware,
      outputDir: destination,
      target: "macos",
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
      const loaded = await bridge.openProject(path);
      const settings = loaded.settings;
      if (
        !designs.designs.some((design) => design.id === settings.theme) ||
        !designs.palettes.some((value) => value.id === settings.palette) ||
        settings.target !== "macos"
      ) {
        throw new Error(
          "This project uses a menu design, palette or platform unavailable in this build.",
        );
      }
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
        splash: settings.splash ?? false,
        advancedEmulatorAccess: settings.advancedEmulatorAccess ?? false,
        startAtMenu: settings.startAtMenu,
      });
      setPalette(settings.palette);
      setMenuSounds(settings.menuSounds || "off");
      setControls(settings.controls || emptyControls());
      setFirmware(settings.firmware || []);
      setBundledShaders(settings.shaders?.bundled ?? []);
      setCustomShaders(settings.shaders?.custom ?? []);
      setShaderInitial(settings.shaders?.initial ?? null);
      setInfo({
        title: settings.title,
        system: settings.system,
        filename: settings.rom.split(/[\\/]/).pop() || settings.title,
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
  async function packageGame() {
    if (!selection || busy || !bridge.native) return;
    setError("");
    setBusy("export");
    setProgress({
      stage: "preparing",
      fraction: 0,
      message: "Preparing your game…",
    });
    try {
      const value = await bridge.exportGame(exportRequest());
      setResult(value);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
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
    else if (step === 0)
      choose({
        path: paths[0],
        name: paths[0].split(/[\\/]/).pop() || paths[0],
      });
  };
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
    bridge.onExportProgress(setProgress).then(save).catch(fail);
    bridge.defaultDestination().then(setDestination).catch(fail);
    bridge
      .availableSystems()
      .then((ids) => setSupported(new Set(ids)))
      .catch(fail);
    return () => {
      disposed = true;
      cleanups.forEach((fn) => fn());
    };
  }, []);
  useEffect(() => {
    setSavedProject("");
  }, [draft, icon, background, palette, menuSounds, controls, firmware]);
  const soundCharacter =
    designs.soundPacks.find((pack) => pack.id === menuSounds)?.description ??
    "";
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
    if (step !== 2 || !draft.showMenu) return;
    if (!bridge.native) {
      setPreview(`/native-menu-${palette}.png`);
      return;
    }
    let cancelled = false;
    setPreviewBusy(true);
    setPreviewError("");
    bridge
      .menuPreview(background?.path || null, palette)
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
  }, [step, draft.showMenu, background?.path, palette]);
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
  // Some consoles cannot start without a BIOS from the author, and others run
  // without one and only work better with it. If we showed both the same way,
  // we would either ask for a file nobody needs or make an export that cannot
  // work look ready. `minimum` is the number of files required in that group.
  const mandatoryFirmware = requirements.filter((group) => group.minimum > 0);
  const missingMandatory = mandatoryFirmware.filter(
    (group) =>
      firmware.filter((path) =>
        group.acceptedNames.some(
          (name) =>
            name.toLowerCase() ===
            (path.split(/[\\/]/).pop() || "").toLowerCase(),
        ),
      ).length < group.minimum,
  );
  const requiresFirmware = requirements.length > 0;
  const firmwarePicker = (
    <div className="firmware-picker">
      <button
        type="button"
        className="secondary"
        disabled={!bridge.native}
        onClick={async () => {
          try {
            const paths = await bridge.pickFirmware();
            setFirmware((current) => [...new Set([...current, ...paths])]);
          } catch (e) {
            fail(e);
          }
        }}
      >
        {mandatoryFirmware.length > 0 && firmware.length === 0
          ? "Choose BIOS files"
          : "Add BIOS files"}
      </button>
      <Help>
        Choose the BIOS files for this console. Only these files are bundled.
      </Help>
      {firmware.map((path) => (
        <div className="firmware-file" key={path}>
          <span>{path.split(/[\\/]/).pop()}</span>
          <button
            type="button"
            className="text-button"
            aria-label={"Remove " + path.split(/[\\/]/).pop()}
            onClick={() =>
              setFirmware((current) => current.filter((file) => file !== path))
            }
          >
            Remove
          </button>
        </div>
      ))}
    </div>
  );
  const validDetails = !!draft.title.trim() && !!draft.system;
  const stepAvailable = (index: number) =>
    index === 0 || (index === 1 ? !!selection : !!info && validDetails);
  const canNext =
    step === 0 ? !!selection : step === 1 ? !!info && validDetails : true;
  const progressPercent = Math.round(
    Math.max(0, Math.min(1, progress?.fraction || 0)) * 100,
  );
  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="wordmark">
          <span className="brand-mark" aria-hidden="true">
            R<span>■</span>
          </span>
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
                <Cartridge />
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
                    help="Use established game catalogs and artwork. Artwork requests send the matched title to GitHub. The ROM stays on your computer."
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
                {info && validDetails && (
                  <span className="ready">
                    <Check size={17} />
                    Ready to go
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
                          <Cartridge />
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
                            <option key={s.id} value={s.id}>
                              {s.name}
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
                  {requiresFirmware && (
                    <div
                      className={
                        missingMandatory.length > 0
                          ? "firmware-required"
                          : "firmware-optional"
                      }
                    >
                      <p className="firmware-status">
                        {missingMandatory.length > 0
                          ? missingMandatory[0].help
                          : mandatoryFirmware.length > 0
                            ? "BIOS files added."
                            : requirements[0].help}
                      </p>
                      {firmwarePicker}
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
                          value="native"
                          onChange={() => {}}
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
                    {" "}
                    <Checkbox
                      className="splash-choice"
                      label="Startup logo"
                      checked={draft.splash}
                      onChange={(value) => update("splash", value)}
                      help="Show a brief ROM-in-a-Box logo in the game window at startup."
                    />{" "}
                    <Checkbox
                      label="Show menu at startup"
                      checked={draft.startAtMenu}
                      onChange={(value) => update("startAtMenu", value)}
                      help="Start at the menu before playing. The menu is also available during play."
                    />
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
                            <option
                              value={pack.id}
                              key={pack.id}
                              title={pack.description}
                            >
                              {pack.name}
                            </option>
                          ))}
                        </select>
                        <MenuSoundPreview pack={menuSounds} />
                        <Help>
                          Each pack is one complete set of navigation, confirm
                          and back cues. Only the selected pack is bundled.
                        </Help>
                        <span className="sound-character">
                          {soundCharacter}
                        </span>
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
                  <Cartridge />
                  <p>No in-game menu.</p>
                </div>
              )}
              {!draft.showMenu && (
                <Checkbox
                  className="splash-choice"
                  label="Startup logo"
                  checked={draft.splash}
                  onChange={(value) => update("splash", value)}
                  help="Show a brief ROM-in-a-Box logo in the game window at startup."
                />
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
                <div className="shader-choices">
                  {shaderCatalog.presets.map((preset) => (
                    <Checkbox
                      key={preset.id}
                      label={preset.name}
                      checked={bundledShaders.includes(preset.id)}
                      onChange={(value) => {
                        setBundledShaders((current) =>
                          value
                            ? [...current, preset.id]
                            : current.filter((id) => id !== preset.id),
                        );
                        if (!value) {
                          setShaderInitial((current) =>
                            current === preset.id ? null : current,
                          );
                        }
                      }}
                      help={preset.detail}
                    />
                  ))}
                  {customShaders.map((shader) => (
                    <div key={shader.path} className="custom-shader">
                      <span>{shader.name}</span>
                      <button
                        type="button"
                        onClick={() => {
                          setCustomShaders((current) =>
                            current.filter((item) => item.path !== shader.path),
                          );
                          setShaderInitial((current) =>
                            current === shader.name ? null : current,
                          );
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={() => {
                      void bridge.pickShader().then((path) => {
                        if (!path) return;
                        const name =
                          path
                            .split(/[\\/]/)
                            .pop()
                            ?.replace(/\.(glslp|glsl)$/i, "") || "Shader";
                        setCustomShaders((current) => {
                          if (current.some((item) => item.path === path)) {
                            return current;
                          }
                          return [...current, { name, path }];
                        });
                      });
                    }}
                  >
                    Add shader
                  </button>
                  {bundledShaders.length + customShaders.length > 0 && (
                    <label>
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
                  {icon ? <img src={icon.url} alt="" /> : <Cartridge />}
                </div>
                <div>
                  <h2>{draft.title}</h2>
                  <p>{systemName}</p>
                  <span className="export-format">MACOS APP + ZIP</span>
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
                      <dt>Download</dt>
                      <dd>{formatBytes(result.archiveBytes)}</dd>
                    </div>
                    <div>
                      <dt>Installed</dt>
                      <dd>{formatBytes(result.installedBytes)}</dd>
                    </div>
                  </dl>
                  <button
                    className="secondary"
                    onClick={() =>
                      bridge.reveal(result.archivePath).catch(fail)
                    }
                  >
                    <FolderOpen size={18} />
                    Show files
                  </button>
                </div>
              ) : (
                <>
                  <div className="destination">
                    <label>
                      Save to
                      <Help>
                        The app and ZIP are saved together. An existing export
                        will not be silently replaced.
                      </Help>
                    </label>
                    <button
                      onClick={async () => {
                        if (!bridge.native) return;
                        try {
                          const folder = await bridge.pickFolder();
                          if (folder) setDestination(folder);
                        } catch (e) {
                          fail(e);
                        }
                      }}
                    >
                      <FolderOpen size={19} />
                      <span>{destination || "Downloads / ROM-in-a-Box"}</span>
                      <span>Change</span>
                    </button>
                  </div>
                  {bridge.native &&
                    !supported.has(systemDefinition?.id || draft.system) && (
                      <p className="error">
                        This build does not include the {systemName} core yet.
                      </p>
                    )}
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
                !!busy ||
                !bridge.native ||
                !supported.has(systemDefinition?.id || draft.system) ||
                !destination
              }
              onClick={packageGame}
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
