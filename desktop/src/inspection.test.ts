import { describe, expect, it, vi } from "vitest";

import { formatBytes, inspectRom, SYSTEMS } from "./inspection";

function romFile(name: string, bytes: Uint8Array): File {
  return new File([bytes.slice().buffer as ArrayBuffer], name);
}

function writeAscii(bytes: Uint8Array, offset: number, value: string): void {
  [...value].forEach((character, index) => {
    bytes[offset + index] = character.charCodeAt(0);
  });
}

function finishGameBoyHeader(bytes: Uint8Array): void {
  bytes.set(
    [
      0xce, 0xed, 0x66, 0x66, 0xcc, 0x0d, 0x00, 0x0b, 0x03, 0x73, 0x00, 0x83,
      0x00, 0x0c, 0x00, 0x0d, 0x00, 0x08, 0x11, 0x1f, 0x88, 0x89, 0x00, 0x0e,
      0xdc, 0xcc, 0x6e, 0xe6, 0xdd, 0xdd, 0xd9, 0x99, 0xbb, 0xbb, 0x67, 0x63,
      0x6e, 0x0e, 0xec, 0xcc, 0xdd, 0xdc, 0x99, 0x9f, 0xbb, 0xb9, 0x33, 0x3e,
    ],
    0x104,
  );
  let checksum = 0;
  for (let index = 0x134; index <= 0x14c; index += 1)
    checksum = (checksum - bytes[index] - 1) & 0xff;
  bytes[0x14d] = checksum;
}

describe("inspectRom", () => {
  it("uses a sanitized Mega Drive header title", async () => {
    const bytes = new Uint8Array(512);
    writeAscii(bytes, 0x100, "SEGA");
    writeAscii(bytes, 0x150, "  SONIC\u0001 THE HEDGEHOG  ");

    await expect(
      inspectRom(romFile("upload.gen", bytes)),
    ).resolves.toMatchObject({
      title: "SONIC THE HEDGEHOG",
      system: "megadrive",
      source: "header",
      filename: "upload.gen",
      size: 512,
    });
  });

  it("recognizes a valid Mega Drive header with a generic bin extension", async () => {
    const bytes = new Uint8Array(512);
    writeAscii(bytes, 0x100, "SEGA");
    writeAscii(bytes, 0x150, "STREETS OF RAGE");

    await expect(
      inspectRom(romFile("cartridge.bin", bytes)),
    ).resolves.toMatchObject({
      title: "STREETS OF RAGE",
      system: "megadrive",
      source: "header",
    });
  });

  it("uses Game Boy Color flags and header titles", async () => {
    const bytes = new Uint8Array(512);
    writeAscii(bytes, 0x134, "POKEMON CRYSTAL");
    bytes[0x143] = 0xc0;
    finishGameBoyHeader(bytes);

    await expect(
      inspectRom(romFile("pokemon.gb", bytes)),
    ).resolves.toMatchObject({
      title: "POKEMON CRYSTAL",
      system: "gbc",
      source: "header",
    });
  });

  it("recognizes a Game Boy Advance header in a generic binary", async () => {
    const bytes = new Uint8Array(512);
    bytes.set([0x24, 0xff, 0xae, 0x51], 4);
    writeAscii(bytes, 0xa0, "ADVANCE TEST");
    bytes[0xb2] = 0x96;

    await expect(
      inspectRom(romFile("upload.bin", bytes)),
    ).resolves.toMatchObject({
      title: "ADVANCE TEST",
      system: "gba",
      source: "header",
    });
  });

  it("falls back to a cleaned filename", async () => {
    const result = await inspectRom(
      romFile("  My   Game  .gba", new Uint8Array(32)),
    );
    expect(result).toMatchObject({
      title: "My Game",
      system: "gba",
      source: "filename",
    });
  });

  it("does not trust a Game Boy extension without a valid header", async () => {
    const result = await inspectRom(
      romFile("not-a-rom.gb", new Uint8Array(512)),
    );
    expect(result.system).toBe("");
  });

  it.each(["archive.cue", "archive.chd", "archive.iso"])(
    "leaves the ambiguous disc image %s for user choice",
    async (name) => {
      const result = await inspectRom(romFile(name, new Uint8Array(32)));
      expect(result.system).toBe("");
    },
  );

  it("accepts an explicit system override for an ambiguous disc image", async () => {
    const result = await inspectRom(
      romFile("archive.cue", new Uint8Array(32)),
      undefined,
      "segacd",
    );
    expect(result.system).toBe("segacd");
  });

  it("reads at most the first 512 bytes of a large file", async () => {
    const header = new Uint8Array(512);
    const arrayBuffer = vi.fn(async () => header.buffer as ArrayBuffer);
    const slice = vi.fn((start?: number, end?: number) => {
      expect(start).toBe(0);
      expect(end).toBe(512);
      return { arrayBuffer } as unknown as Blob;
    });
    const file = {
      name: "large.iso",
      size: 8 * 1024 ** 3,
      slice,
    } as unknown as File;

    await inspectRom(file);
    expect(slice).toHaveBeenCalledOnce();
    expect(arrayBuffer).toHaveBeenCalledOnce();
  });

  it("rejects empty and unsupported files", async () => {
    await expect(
      inspectRom(romFile("empty.gbc", new Uint8Array())),
    ).rejects.toThrow("Choose a non-empty game file.");
    await expect(
      inspectRom(romFile("notes.txt", new Uint8Array(1))),
    ).rejects.toThrow("Choose a supported game file.");
  });
});

describe("inspection declarations", () => {
  it("keeps the domain systems and readable byte sizes", () => {
    expect(SYSTEMS.find((system) => system.id === "megadrive")).toMatchObject({
      id: "megadrive",
      name: "Mega Drive / Genesis",
      extensions: [".md", ".gen", ".smd"],
      controllerProfile: "megadrive",
      cores: [
        expect.objectContaining({
          // A core is not one file. The registry has an artifact per target,
          // so a Windows build is a declaration and not a second code path.
          artifacts: expect.objectContaining({
            "macos-arm64": "genesis_plus_gx_libretro.dylib",
          }),
          license: "Non-commercial",
        }),
      ],
    });
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1.5 * 1024 ** 2)).toBe("1.50 MB");
  });

  // N64 declares a header window too, and this covers it in the browser
  // preview.
  it("reads a Nintendo 64 header title from its declared window", async () => {
    const bytes = new Uint8Array(0x100);
    bytes.set([0x80, 0x37, 0x12, 0x40], 0);
    const title = "SUPER MARIO 64";
    for (let index = 0; index < title.length; index += 1)
      bytes[0x20 + index] = title.charCodeAt(index);

    const inspection = await inspectRom(romFile("cart.z64", bytes));
    expect(inspection.system).toBe("n64");
    expect(inspection.source).toBe("header");
    expect(inspection.title).toBe("SUPER MARIO 64");
  });
});
