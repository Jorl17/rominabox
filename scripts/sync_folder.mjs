// Make folder TO contain exactly the files of folder FROM, and write only the
// files whose bytes differ, so that an unchanged file keeps its time. We embed
// the frontend build in the builder. After a Vite build every file is new, and
// with Cargo we compile the builder again when an embedded file is newer.
//
//   node scripts/sync_folder.mjs FROM TO
import fs from "node:fs";
import path from "node:path";

function files(root, below = "") {
  if (!fs.existsSync(path.join(root, below))) return [];
  return fs.readdirSync(path.join(root, below), { withFileTypes: true }).flatMap((entry) => {
    const name = path.join(below, entry.name);
    return entry.isDirectory() ? files(root, name) : [name];
  });
}

const [from, to] = process.argv.slice(2);
if (!from || !to) {
  console.error("usage: node scripts/sync_folder.mjs FROM TO");
  process.exit(2);
}
const made = new Set(files(from));
for (const name of made) {
  const bytes = fs.readFileSync(path.join(from, name));
  const target = path.join(to, name);
  if (fs.existsSync(target) && bytes.equals(fs.readFileSync(target))) continue;
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, bytes);
}
for (const name of files(to)) {
  if (!made.has(name)) fs.rmSync(path.join(to, name));
}
