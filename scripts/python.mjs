// The Python we run helper scripts with from a Node or npm script, which is
// the interpreter scripts/test.py was started with (ROMINABOX_PYTHON), or
// else uv's, with the version in .python-version and the packages pinned in
// uv.lock, as in every command of the README.
//
//   node scripts/python.mjs SCRIPT [ARGS...]   runs SCRIPT with that Python
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

// The program and the arguments that come before the script's own.
// python(...args) returns the whole command.
export function python(...args) {
  if (process.env.ROMINABOX_PYTHON) return [process.env.ROMINABOX_PYTHON, ...args];
  return ["uv", "run", "--locked", "--exact", "--project", ROOT, "python", ...args];
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [program, ...args] = python(...process.argv.slice(2));
  const ran = spawnSync(program, args, { stdio: "inherit" });
  if (ran.error) {
    console.error(`${program}: ${ran.error.message}`);
    process.exit(127);
  }
  process.exit(ran.status ?? 1);
}
