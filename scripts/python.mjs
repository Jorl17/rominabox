// The Python we run helper scripts with from a Node or npm script, which is
// the one scripts/test.py was started with (ROMINABOX_PYTHON), or else the
// usual name. That is `python` on Windows, where `python3` is the Microsoft
// Store stub, and `python3` on macOS, Linux and other POSIX systems.
//
//   node scripts/python.mjs SCRIPT [ARGS...]   runs SCRIPT with that Python
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

export function python() {
  if (process.env.ROMINABOX_PYTHON) return process.env.ROMINABOX_PYTHON;
  return process.platform === "win32" ? "python" : "python3";
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const ran = spawnSync(python(), process.argv.slice(2), { stdio: "inherit" });
  if (ran.error) {
    console.error(`${python()}: ${ran.error.message}`);
    process.exit(127);
  }
  process.exit(ran.status ?? 1);
}
