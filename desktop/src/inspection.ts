import catalog from "../systems.json";

type HeaderWindow = {
  offset: number;
  length: number;
  anchor?: string;
  complementAt?: number;
  magic?: { offset: number; text?: string; hex?: string };
};

/**
 * The header windows a console declares, from the generated registry.
 *
 * The browser preview still has separate signature detection, but we declare
 * the title offsets once, in the console packages, because a second copy
 * here would drift apart from them. A console may declare one window or
 * several, and Super Nintendo has the title at two addresses.
 */
function headerWindows(systemId: string): HeaderWindow[] {
  const system = catalog.systems.find((entry) => entry.id === systemId);
  const raw = system?.headerTitle as HeaderWindow | HeaderWindow[] | undefined;
  if (!raw) return [];
  return Array.isArray(raw) ? raw : [raw];
}

const WEAK_FILENAMES = new Set([
  "GAME",
  "ROM",
  "CART",
  "UPLOAD",
  "UNTITLED",
  "CARTRIDGE",
  "FILE",
  "IMAGE",
  "DUMP",
  "DISC",
  "TRACK",
]);

function alnumUpper(value: string): string {
  return [...value]
    .filter((character) => /[0-9a-z]/i.test(character))
    .join("")
    .toUpperCase();
}

/** Same rule as the native inspector: a complete filename beats a cut-off header. */
function headerBeatsFilename(
  filenameTitle: string,
  headerTitle: string,
): boolean {
  const fileKey = alnumUpper(filenameTitle);
  const headKey = alnumUpper(headerTitle);
  const letters = [...headKey].filter(
    (character) => character >= "A" && character <= "Z",
  ).length;
  if (letters < 3) return false;
  if (fileKey.length < 4 || WEAK_FILENAMES.has(fileKey)) return true;
  let common = 0;
  while (
    common < fileKey.length &&
    common < headKey.length &&
    fileKey[common] === headKey[common]
  ) {
    common += 1;
  }
  if (common >= 8 && fileKey.length > headKey.length) return false;
  if (headKey.startsWith(fileKey) && headKey.length > fileKey.length)
    return true;
  return common < 4 && headKey.length > fileKey.length;
}

function magicMatches(
  header: Uint8Array,
  magic: NonNullable<HeaderWindow["magic"]>,
): boolean {
  if (magic.text) {
    for (let index = 0; index < magic.text.length; index += 1) {
      if (header[magic.offset + index] !== magic.text.charCodeAt(index))
        return false;
    }
  }
  if (magic.hex) {
    if (magic.hex.length % 2 !== 0) return false;
    for (let index = 0; index < magic.hex.length; index += 2) {
      const byte = Number.parseInt(magic.hex.slice(index, index + 2), 16);
      if (header[magic.offset + index / 2] !== byte) return false;
    }
  }
  return Boolean(magic.text || magic.hex);
}

function complementMatches(header: Uint8Array, at: number): boolean {
  if (at + 4 > header.length) return false;
  const checksum = header[at] | (header[at + 1] << 8);
  const complement = header[at + 2] | (header[at + 3] << 8);
  return ((checksum + complement) & 0xffff) === 0xffff;
}

function declaredTitle(header: Uint8Array, systemId: string): string {
  for (const window of headerWindows(systemId)) {
    if (window.anchor) continue;
    if (window.magic && !magicMatches(header, window.magic)) continue;
    const start = window.offset;
    if (
      window.complementAt != null &&
      !complementMatches(header, start + window.complementAt)
    ) {
      continue;
    }
    const title = asciiTitle(header, start, start + window.length);
    if (title) return title;
  }
  return "";
}

function bytesToRead(
  filename: string,
  size: number,
): { skip: number; length: number } {
  const extension = extensionOf(filename);
  const candidates = SYSTEMS.filter((system) =>
    (system.extensions as readonly string[]).includes(extension),
  );
  if (candidates.length !== 1) return { skip: 0, length: HEADER_BYTES };
  const system = candidates[0];
  const declared = "copierHeader" in system ? Number(system.copierHeader) : 0;
  const copier =
    Number.isFinite(declared) &&
    declared > 0 &&
    size > declared &&
    size % 1024 === declared
      ? declared
      : 0;
  let length = HEADER_BYTES;
  for (const window of headerWindows(system.id)) {
    if (window.anchor) continue;
    length = Math.max(length, window.offset + window.length);
    if (window.complementAt != null) {
      length = Math.max(length, window.offset + window.complementAt + 4);
    }
  }
  return { skip: copier, length };
}

export type SystemDeclaration = (typeof catalog.systems)[number];
export const SYSTEM_CATALOG = catalog.systems;
// Keep the UI-facing extension format stable while loading declarations from
// the catalog shared with the native engine.
export const SYSTEMS = SYSTEM_CATALOG.map((system) => ({
  ...system,
  extensions: system.extensions.map((extension) => `.${extension}`),
}));

export type Inspection = {
  title: string;
  system: string;
  source: "header" | "filename";
  filename: string;
  size: number;
};

const HEADER_BYTES = 512;
const INVALID_TITLE_CHARACTERS = /[<>:"/\\|?*\x00-\x1f]/g;
const GAME_BOY_LOGO = [
  0xce, 0xed, 0x66, 0x66, 0xcc, 0x0d, 0x00, 0x0b, 0x03, 0x73, 0x00, 0x83, 0x00,
  0x0c, 0x00, 0x0d, 0x00, 0x08, 0x11, 0x1f, 0x88, 0x89, 0x00, 0x0e, 0xdc, 0xcc,
  0x6e, 0xe6, 0xdd, 0xdd, 0xd9, 0x99, 0xbb, 0xbb, 0x67, 0x63, 0x6e, 0x0e, 0xec,
  0xcc, 0xdd, 0xdc, 0x99, 0x9f, 0xbb, 0xb9, 0x33, 0x3e,
];

function extensionOf(filename: string): string {
  const dot = filename.lastIndexOf(".");
  return dot < 0 ? "" : filename.slice(dot).toLowerCase();
}

function cleanTitle(value: string): string {
  const cleaned = value
    .replace(INVALID_TITLE_CHARACTERS, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/^[. ]+|[. ]+$/g, "");
  return cleaned.slice(0, 100) || "Untitled game";
}

function filenameTitle(filename: string): string {
  const extension = extensionOf(filename);
  return cleanTitle(
    extension ? filename.slice(0, -extension.length) : filename,
  );
}

function asciiTitle(
  header: Uint8Array,
  start: number,
  end: number,
): string | undefined {
  const bytes = header.slice(start, Math.min(end, header.length));
  let value = "";
  for (const byte of bytes) {
    if (byte === 0) break;
    value += byte >= 0x20 && byte <= 0x7e ? String.fromCharCode(byte) : " ";
  }
  const printable = value
    .replace(INVALID_TITLE_CHARACTERS, " ")
    .replace(/\s+/g, " ")
    .trim();
  return printable ? cleanTitle(printable) : undefined;
}

function matches(
  header: Uint8Array,
  offset: number,
  expected: string,
): boolean {
  return [...expected].every(
    (character, index) => header[offset + index] === character.charCodeAt(0),
  );
}

function hasValidGameBoyHeader(header: Uint8Array): boolean {
  if (
    header.length <= 0x14d ||
    !GAME_BOY_LOGO.every((byte, index) => header[0x104 + index] === byte)
  )
    return false;
  let checksum = 0;
  for (let index = 0x134; index <= 0x14c; index += 1)
    checksum = (checksum - header[index] - 1) & 0xff;
  return checksum === header[0x14d];
}

export async function inspectRom(
  file: File,
  onProgress?: (label: string) => void,
  systemOverride?: string,
): Promise<Inspection> {
  if (file.size === 0) throw new Error("Choose a non-empty game file.");

  const extension = extensionOf(file.name);
  const candidates = SYSTEMS.filter((system) =>
    (system.extensions as readonly string[]).includes(extension),
  );

  onProgress?.("Inspecting game file…");
  const span = bytesToRead(file.name, file.size);
  const header = new Uint8Array(
    await file.slice(span.skip, span.skip + span.length).arrayBuffer(),
  );
  const overridden = systemOverride
    ? SYSTEMS.find(
        (candidate) =>
          candidate.id.toLowerCase() === systemOverride.toLowerCase() ||
          candidate.aliases.some(
            (alias) => alias.toLowerCase() === systemOverride.toLowerCase(),
          ),
      )
    : undefined;
  if (systemOverride && !overridden)
    throw new Error(`Unknown console: ${systemOverride}`);
  if (overridden && !overridden.extensions.includes(extension))
    throw new Error(`${overridden.name} does not support ${extension} files.`);

  let system =
    overridden?.id ?? (candidates.length === 1 ? candidates[0].id : "");
  let title = filenameTitle(file.name);
  let source: Inspection["source"] = "filename";

  if (!overridden && matches(header, 0x100, "SEGA")) {
    system = "megadrive";
  } else if (!overridden && matches(header, 0, "NES\u001a")) {
    system = "nes";
  } else if (
    !overridden &&
    header.length >= 4 &&
    [
      [0x80, 0x37, 0x12, 0x40],
      [0x37, 0x80, 0x40, 0x12],
      [0x40, 0x12, 0x37, 0x80],
    ].some((magic) => magic.every((byte, index) => header[index] === byte))
  ) {
    system = "n64";
  } else if (!overridden && matches(header, 0, "LYNX")) {
    system = "lynx";
  } else if (!overridden && matches(header, 1, "ATARI7800")) {
    system = "atari7800";
  } else if (
    !overridden &&
    header[4] === 0x24 &&
    header[5] === 0xff &&
    header[6] === 0xae &&
    header[7] === 0x51 &&
    header[0xb2] === 0x96
  ) {
    system = "gba";
  } else if (!overridden && hasValidGameBoyHeader(header)) {
    const colorFlag = header[0x143];
    system = colorFlag === 0x80 || colorFlag === 0xc0 ? "gbc" : "gb";
  } else if (!overridden && (extension === ".gb" || extension === ".gbc")) {
    system = "";
  }

  if (system) {
    const fromHeader = declaredTitle(header, system);
    if (fromHeader && headerBeatsFilename(title, fromHeader)) {
      title = fromHeader;
      source = "header";
    }
  }

  if (candidates.length === 0 && !system)
    throw new Error("Choose a supported game file.");

  onProgress?.("Game file inspected.");
  return { title, system, source, filename: file.name, size: file.size };
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = -1;
  do {
    value /= 1024;
    unit += 1;
  } while (value >= 1024 && unit < units.length - 1);
  return `${value.toFixed(value >= 10 ? 1 : 2).replace(/\.0+$/, "")} ${units[unit]}`;
}
