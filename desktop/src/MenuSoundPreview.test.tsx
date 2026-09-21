import { act, useLayoutEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { build, type Rollup } from "vite";

import bellUp from "../assets/menu-sounds/bell/up.wav";
import blipCancel from "../assets/menu-sounds/blip/cancel.wav";
import blipDown from "../assets/menu-sounds/blip/down.wav";
import blipOk from "../assets/menu-sounds/blip/ok.wav";
import blipUp from "../assets/menu-sounds/blip/up.wav";
import designs from "../designs.json";
import tauriConf from "../src-tauri/tauri.conf.json";
import viteConfig from "../vite.config";
import { CUES, MenuSoundPreview, PREVIEWABLE_PACKS } from "./MenuSoundPreview";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let playImpl: (audio: FakeAudio) => Promise<void> = async () => undefined;

class FakeAudio {
  src: string;
  currentTime = 0;
  paused = true;
  lastError: (() => void) | null = null;
  #onended: (() => void) | null = null;
  #onerror: (() => void) | null = null;
  play = vi.fn(() => {
    this.paused = false;
    return playImpl(this);
  });
  pause = vi.fn(() => {
    this.paused = true;
  });
  load = vi.fn();
  removeAttribute = vi.fn((name: string) => {
    if (name === "src") this.src = "";
  });

  get onended(): (() => void) | null {
    return this.#onended;
  }

  set onended(handler: (() => void) | null) {
    this.#onended = handler;
  }

  get onerror(): (() => void) | null {
    return this.#onerror;
  }

  set onerror(handler: (() => void) | null) {
    this.#onerror = handler;
    if (handler) this.lastError = handler;
  }

  constructor(src = "") {
    this.src = src;
    created.push(this);
  }
}

const created: FakeAudio[] = [];
let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  created.length = 0;
  playImpl = async () => undefined;
  vi.stubGlobal("Audio", FakeAudio);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

function renderPreview(pack: string): void {
  act(() => root.render(<MenuSoundPreview pack={pack} />));
}

function previewButton(): HTMLButtonElement {
  const match =
    container.querySelector('[aria-label="Preview menu sounds"]') ||
    container.querySelector('[aria-label="Stop"]');
  if (!(match instanceof HTMLButtonElement))
    throw new Error("Missing preview button");
  return match;
}

function clickPreview(): void {
  act(() => previewButton().click());
}

async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

function finish(audio: FakeAudio): void {
  act(() => audio.onended?.());
}

async function waitForUnhandled(): Promise<void> {
  await flush();
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
}

async function withUnhandled(
  run: (leaked: unknown[]) => Promise<void>,
): Promise<void> {
  const leaked: unknown[] = [];
  const onUnhandled = (event: PromiseRejectionEvent) => {
    leaked.push(event.reason);
    event.preventDefault();
  };
  window.addEventListener("unhandledrejection", onUnhandled);
  try {
    await run(leaked);
  } finally {
    window.removeEventListener("unhandledrejection", onUnhandled);
  }
}

describe("menu sound preview", () => {
  it("disables Off and unknown packs without guessing or playing", () => {
    renderPreview("off");
    expect(previewButton().disabled).toBe(true);
    expect(previewButton().getAttribute("title")).toBe("Preview menu sounds");
    clickPreview();
    expect(created).toHaveLength(0);

    renderPreview("missing");
    expect(previewButton().disabled).toBe(true);
    clickPreview();
    expect(created).toHaveLength(0);
  });

  it("plays a bundled sequence from a user gesture and stops on click, pack change, and unmount", async () => {
    renderPreview("blip");
    const idle = previewButton();
    expect(idle.disabled).toBe(false);
    expect(idle.getAttribute("aria-label")).toBe("Preview menu sounds");
    expect(idle.getAttribute("title")).toBe("Preview menu sounds");
    const idleBox = idle.getBoundingClientRect();

    clickPreview();
    await flush();
    const stop = previewButton();
    expect(stop.getAttribute("aria-label")).toBe("Stop");
    expect(stop.getAttribute("title")).toBe("Stop");
    expect(stop.getBoundingClientRect().width).toBe(idleBox.width);
    expect(stop.getBoundingClientRect().height).toBe(idleBox.height);
    expect(created).toHaveLength(1);
    expect(created[0].src).toBe(blipUp);
    expect(created[0].play).toHaveBeenCalledTimes(1);

    clickPreview();
    expect(created[0].pause).toHaveBeenCalled();
    expect(created[0].src).toBe("");
    expect(previewButton().getAttribute("aria-label")).toBe(
      "Preview menu sounds",
    );
    finish(created[0]);
    await flush();
    expect(created).toHaveLength(1);

    clickPreview();
    await flush();
    expect(created).toHaveLength(2);
    expect(created[1].src).toBe(blipUp);
    finish(created[1]);
    await flush();
    expect(created).toHaveLength(3);
    expect(created[2].src).toBe(blipDown);

    act(() => root.render(<MenuSoundPreview pack="bell" />));
    expect(previewButton().getAttribute("aria-label")).toBe(
      "Preview menu sounds",
    );
    expect(created[2].pause).toHaveBeenCalled();
    const afterChange = created.length;
    finish(created[2]);
    await flush();
    expect(created).toHaveLength(afterChange);

    clickPreview();
    await flush();
    expect(created[afterChange].src).toBe(bellUp);

    act(() => root.unmount());
    expect(created[afterChange].pause).toHaveBeenCalled();
    finish(created[afterChange]);
    await flush();
    expect(created).toHaveLength(afterChange + 1);
  });

  it("reports a brief status when playback fails and does not leak a rejection", async () => {
    await withUnhandled(async (leaked) => {
      renderPreview("blip");
      playImpl = () =>
        Promise.reject(new DOMException("failed", "NotSupportedError"));
      clickPreview();
      await flush();
      expect(container.querySelector('[role="status"]')?.textContent).toBe(
        "Could not play.",
      );
      expect(previewButton().getAttribute("aria-label")).toBe(
        "Preview menu sounds",
      );
      expect(leaked).toEqual([]);
    });
  });

  it("fetches packaged Tauri cues and plays them from a blob URL", async () => {
    const objectUrl = "blob:tauri://localhost/blip-up";
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      expect(String(input)).toBe(blipUp);
      return new Response(
        Uint8Array.from([
          0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x41, 0x56, 0x45,
        ]),
      );
    });
    vi.stubGlobal("fetch", fetchImpl);
    vi.stubGlobal("location", {
      ...window.location,
      origin: "tauri://localhost",
      protocol: "tauri:",
    });
    const createObjectURL = vi
      .spyOn(URL, "createObjectURL")
      .mockReturnValue(objectUrl);
    const revokeObjectURL = vi
      .spyOn(URL, "revokeObjectURL")
      .mockImplementation(() => undefined);

    renderPreview("blip");
    clickPreview();
    await flush();
    expect(fetchImpl).toHaveBeenCalledWith(blipUp);
    expect(created).toHaveLength(1);
    expect(created[0].src).toBe(objectUrl);
    expect(created[0].play).toHaveBeenCalledTimes(1);

    clickPreview();
    expect(revokeObjectURL).toHaveBeenCalledWith(objectUrl);
    expect(previewButton().getAttribute("aria-label")).toBe(
      "Preview menu sounds",
    );
    createObjectURL.mockRestore();
    revokeObjectURL.mockRestore();
  });

  it("plays the bundled up, down, ok, cancel clips in order", async () => {
    const expected = [blipUp, blipDown, blipOk, blipCancel];
    renderPreview("blip");
    clickPreview();
    await flush();
    for (const [index, src] of expected.entries()) {
      expect(created).toHaveLength(index + 1);
      expect(created[index].src).toBe(src);
      expect(created[index].play).toHaveBeenCalledTimes(1);
      finish(created[index]);
      await flush();
    }
    expect(created).toHaveLength(4);
    expect(previewButton().getAttribute("aria-label")).toBe(
      "Preview menu sounds",
    );
  });

  it("does not mutate audio while rendering a pack change", async () => {
    let inRender = false;
    function Probe({ pack }: { pack: string }) {
      inRender = true;
      useLayoutEffect(() => {
        inRender = false;
      });
      return <MenuSoundPreview pack={pack} />;
    }

    act(() => root.render(<Probe pack="blip" />));
    clickPreview();
    await flush();
    const audio = created[0];
    const renderMutations: string[] = [];
    audio.pause.mockImplementation(() => {
      if (inRender) renderMutations.push("pause");
      audio.paused = true;
    });
    audio.load.mockImplementation(() => {
      if (inRender) renderMutations.push("load");
    });
    audio.removeAttribute.mockImplementation((name: string) => {
      if (inRender) renderMutations.push("removeAttribute");
      if (name === "src") audio.src = "";
    });

    act(() => root.render(<Probe pack="bell" />));
    expect(renderMutations).toEqual([]);
    expect(audio.pause).toHaveBeenCalled();
    expect(audio.src).toBe("");
    expect(previewButton().getAttribute("aria-label")).toBe(
      "Preview menu sounds",
    );
  });

  it("handles a cue error during play() without leaking a rejection", async () => {
    await withUnhandled(async (leaked) => {
      playImpl = (audio) => {
        queueMicrotask(() => audio.onerror?.());
        return new Promise(() => undefined);
      };
      renderPreview("blip");
      clickPreview();
      await waitForUnhandled();
      expect(container.querySelector('[role="status"]')?.textContent).toBe(
        "Could not play.",
      );
      expect(previewButton().getAttribute("aria-label")).toBe(
        "Preview menu sounds",
      );
      expect(leaked).toEqual([]);
    });
  });

  it("settles the in-flight cue when cancelled so a late error does not leak", async () => {
    await withUnhandled(async (leaked) => {
      let releasePlay: () => void = () => undefined;
      playImpl = () =>
        new Promise((resolve) => {
          releasePlay = () => resolve();
        });

      renderPreview("blip");
      clickPreview();
      await flush();
      const first = created[0];
      expect(first.lastError).toBeTypeOf("function");

      clickPreview();
      expect(first.pause).toHaveBeenCalled();
      expect(previewButton().getAttribute("aria-label")).toBe(
        "Preview menu sounds",
      );

      act(() => first.lastError?.());
      await waitForUnhandled();
      expect(leaked).toEqual([]);
      expect(created).toHaveLength(1);
      expect(container.querySelector('[role="status"]')?.textContent).toBe("");

      releasePlay();
      await flush();
      expect(created).toHaveLength(1);
      expect(leaked).toEqual([]);
    });
  });
});

const CUE_NAMES = CUES;
/** `off` is the one entry with no assets, and every other pack has a full set. */
const DECLARED_PACKS = designs.soundPacks
  .map((pack) => pack.id)
  .filter((id) => id !== "off");
const PACKS = PREVIEWABLE_PACKS;

function cspDirective(csp: string, name: string): string[] {
  const body = csp
    .split(";")
    .map((part) => part.trim())
    .find((part) => part.startsWith(`${name} `));
  return body ? body.slice(name.length).trim().split(/\s+/) : [];
}

function wavSignature(bytes: Uint8Array): boolean {
  return (
    bytes.byteLength > 11 &&
    bytes[0] === 0x52 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[3] === 0x46 &&
    bytes[8] === 0x57 &&
    bytes[9] === 0x41 &&
    bytes[10] === 0x56 &&
    bytes[11] === 0x45
  );
}

function builtFiles(
  result: Awaited<ReturnType<typeof build>>,
): Rollup.OutputAsset[] {
  const outputs = Array.isArray(result) ? result : [result];
  return outputs.flatMap((item) =>
    "output" in item ? item.output.filter((file) => file.type === "asset") : [],
  );
}

function builtChunks(
  result: Awaited<ReturnType<typeof build>>,
): Rollup.OutputChunk[] {
  const outputs = Array.isArray(result) ? result : [result];
  return outputs.flatMap((item) =>
    "output" in item ? item.output.filter((file) => file.type === "chunk") : [],
  );
}

function assetBytes(source: string | Uint8Array): Uint8Array {
  return typeof source === "string" ? new TextEncoder().encode(source) : source;
}

describe("packaged menu sound delivery", () => {
  it("offers exactly the declared packs, each a complete cue set", () => {
    expect([...PACKS]).toEqual([...DECLARED_PACKS].sort());
    expect(PACKS.length).toBeGreaterThan(0);
    for (const pack of PACKS) {
      expect(
        pack,
        "authoring variant separator leaked into a pack id",
      ).not.toContain("--");
    }
    // A picker the author can scan, not a list of near-identical entries.
    expect(designs.soundPacks.length).toBeLessThanOrEqual(12);
    const names = designs.soundPacks.map((pack) => pack.name);
    expect(new Set(names).size).toBe(names.length);
  });

  it("allows Tauri protocol audio without depending on data: media", () => {
    const policies = [
      tauriConf.app.security.csp,
      tauriConf.app.security.devCsp,
    ];
    for (const csp of policies) {
      const media = cspDirective(csp, "media-src");
      expect(media).toContain("'self'");
      expect(media).toContain("blob:");
      expect(media).not.toContain("data:");
    }

    const inline = viteConfig.build?.assetsInlineLimit;
    expect(inline).toBeTypeOf("function");
    expect(
      (
        inline as (filePath: string, content: Uint8Array) => boolean | undefined
      )("assets/menu-sounds/blip/up.wav", new Uint8Array(100)),
    ).toBe(false);
  });

  it("emits every pack's WAVs as hashed protocol assets, not data URLs", async () => {
    const result = await build({
      ...viteConfig,
      configFile: false,
      logLevel: "error",
      build: {
        ...viteConfig.build,
        write: false,
        copyPublicDir: false,
      },
    });

    const files = builtFiles(result);
    const wavs = files.filter((file) => file.fileName.endsWith(".wav"));
    const css = files
      .filter((file) => file.fileName.endsWith(".css"))
      .map((file) => String(file.source))
      .join("\n");
    const scripts = builtChunks(result);
    expect(wavs).toHaveLength(PACKS.length * CUE_NAMES.length);
    expect(scripts.length).toBeGreaterThan(0);
    expect(css).toMatch(/\.menu-sound-preview\{[^}]*flex:none/);
    expect(css).toMatch(/\.menu-sound-preview-status\{[^}]*position:absolute/);
    expect(css).toMatch(/\.menu-sound-preview-status\{[^}]*white-space:nowrap/);

    const bundle = scripts.map((file) => file.code).join("\n");
    expect(bundle).not.toMatch(/data:audio/);
    for (const pack of PACKS) {
      expect(bundle).toContain(pack);
    }
    for (const file of wavs) {
      expect(bundle).toContain(file.fileName.replace(/^assets\//, ""));
      expect(wavSignature(assetBytes(file.source)), file.fileName).toBe(true);
    }
  }, 60_000);
});
