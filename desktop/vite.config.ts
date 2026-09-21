import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    // We set this in a worktree so two checkouts can run at once. Unset, it
    // is the default port. strictPort is on, because a clash must fail with an
    // error instead of serving the frontend of another checkout.
    port: Number(process.env.ROMINABOX_VITE_PORT ?? 1420),
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
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
