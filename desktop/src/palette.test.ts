import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import designs from "../designs.json";

/**
 * In the builder we draw in the palette the author picked, not in blue.
 *
 * The leaders, callouts and key labels of the controller scene, and the frame
 * around the menu preview, use the colours of the palette variables, so
 * choosing Amber draws the editor in amber, as in the exported game.
 *
 * A colour of one palette must not be fixed in a stylesheet. The fallback
 * inside a var() is allowed, because we draw it before we apply a palette,
 * and it is the same value that the variable has a moment later.
 */
const here = dirname(fileURLToPath(import.meta.url));

function colours(css: string): string[] {
  const withoutFallbacks = css.replace(
    /var\(--palette-[a-z-]+,\s*#[0-9a-fA-F]{3,8}\)/g,
    "",
  );
  return [...withoutFallbacks.matchAll(/#[0-9a-fA-F]{6}\b/g)].map((m) =>
    m[0].toLowerCase(),
  );
}

const declared = new Set<string>();
for (const palette of designs.palettes) {
  for (const [name, value] of Object.entries(palette)) {
    if (
      name !== "id" &&
      name !== "name" &&
      typeof value === "string" &&
      value.startsWith("#")
    ) {
      declared.add(value.toLowerCase());
    }
  }
  for (const value of Object.values(
    (palette as { tokens?: Record<string, string> }).tokens ?? {},
  )) {
    declared.add(value.toLowerCase());
  }
}

// Every stylesheet of the builder, found by a search and not listed, so we
// check a new stylesheet without its name here.
const sheets = readdirSync(here).filter((name) => name.endsWith(".css"));

describe("the builder's stylesheets", () => {
  it("has some, or this proves nothing", () => {
    expect(sheets.length).toBeGreaterThan(1);
  });

  for (const sheet of sheets) {
    it(`${sheet} fixes no colour that belongs to a palette`, () => {
      const css = readFileSync(resolve(here, sheet), "utf8");
      const stuck = colours(css).filter((colour) => declared.has(colour));
      expect(
        stuck,
        `${sheet} hardcodes ${stuck.join(", ")}, which belong to a palette. ` +
          `Use var(--palette-<role>) so the editor follows what the author chose.`,
      ).toEqual([]);
    });
  }

  it("knows about more than one palette, or it proves nothing", () => {
    expect(designs.palettes.length).toBeGreaterThan(1);
    expect(declared.size).toBeGreaterThan(8);
  });
});
