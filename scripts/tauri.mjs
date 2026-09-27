// Run the Tauri CLI with this checkout's builder identifier and dev server,
// when `scripts/worktree.py env` has set them. In tauri.conf.json the
// identifier is com.rominabox.desktop and the dev URL uses port 1420, so
// otherwise we would store the data of a worktree's builder with the canonical
// checkout's data and load the canonical checkout's dev server in it. Outside
// a worktree we add nothing.
//
//   node ../scripts/tauri.mjs COMMAND [ARGS...]    (npm run tauri, in desktop/)
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const cli = require.resolve("@tauri-apps/cli/tauri.js", { paths: [process.cwd()] });

// The Tauri commands with tauri.conf.json and the --config option.
const CONFIGURED = new Set(["dev", "build"]);

const [command, ...rest] = process.argv.slice(2);
const override = {};
if (process.env.ROMINABOX_BUNDLE_ID) override.identifier = process.env.ROMINABOX_BUNDLE_ID;
if (process.env.ROMINABOX_VITE_PORT) {
  override.build = { devUrl: `http://127.0.0.1:${process.env.ROMINABOX_VITE_PORT}` };
}
const configured = CONFIGURED.has(command) && Object.keys(override).length
  ? ["--config", JSON.stringify(override)]
  : [];

const ran = spawnSync(process.execPath, [cli, ...(command ? [command] : []), ...configured, ...rest], {
  stdio: "inherit",
});
if (ran.error) {
  console.error(`tauri: ${ran.error.message}`);
  process.exit(127);
}
process.exit(ran.status ?? 1);
