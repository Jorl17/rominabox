// Declared here instead of adding @types/node for one lookup. This file runs
// in Node, but the app does not, and we typecheck this config in the build.
declare const process: { env: Record<string, string | undefined> };

import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { defineConfig, type Plugin } from "vitest/config";

/** The repository licences at /licenses/ in the browser preview, where we
 * read them for the About dialog. In the app we read the bundled copy. Paths
 * here are relative to this folder, where Vite and Vitest run. */
function licences(): Plugin {
  return {
    name: "licences",
    configureServer(server) {
      server.middlewares.use("/licenses/", (request, response, next) => {
        const file = decodeURIComponent((request.url ?? "").replace(/^\//, ""));
        if (!/^[a-z0-9-]+(\/[A-Za-z0-9._+-]+)?\.(json|txt)$/.test(file))
          return next();
        readFile(`../licenses/${file}`).then(
          (body) => response.end(body),
          () => next(),
        );
      });
    },
  };
}

/** ROM-in-a-Box's web address, the engine package's homepage. */
const website = /^homepage = "([^"]+)"/m.exec(
  readFileSync("crates/rominabox-engine/Cargo.toml", "utf-8"),
)![1];

export default defineConfig({
  plugins: [react(), licences()],
  define: { __WEBSITE__: JSON.stringify(website) },
  clearScreen: false,
  server: {
    fs: {
      // The shader catalog and the key words are shared with the exporter
      // and the game. We list them here so that those imports are among the
      // files Vite will read.
      allow: [
        ".",
        "../integrations/shaders",
        "../vendor/retroarch/menu/drivers/rmlui",
      ],
    },
    host: "127.0.0.1",
    // We set this in a worktree so two checkouts can run at once. Unset, it
    // is the default port. strictPort is on, because a clash must fail with an
    // error instead of serving the frontend of another checkout.
    port: Number(process.env.ROMINABOX_VITE_PORT ?? 1420),
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    assetsInlineLimit(filePath) {
      if (filePath.endsWith(".wav")) return false;
    },
  },
  test: {
    environment: "jsdom",
  },
});
