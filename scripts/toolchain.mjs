// The native test toolchain for this machine, as we declare it in
// scripts/toolchain.py: { cc, cxx, memoryChecks, executableSuffix }.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { python } from "./python.mjs";

const declaration = fileURLToPath(new URL("./toolchain.py", import.meta.url));

export function toolchain() {
  return JSON.parse(execFileSync(python(), [declaration, "describe"], { encoding: "utf8" }));
}
