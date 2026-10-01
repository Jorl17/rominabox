// Declared here instead of adding @types/node for one lookup. This file runs
// in Node, but the app does not, and we typecheck this config in the build.
declare const process: { env: Record<string, string | undefined> };

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
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
