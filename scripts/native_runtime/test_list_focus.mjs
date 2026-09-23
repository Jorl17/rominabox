// When the pointer is on a list row, we move the yellow highlight there, so
// only one row looks selected. On the controls screen we set focus in one
// function (rib_focus_control). We use that function for every list, the
// shader list included, instead of a special case for discs.
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const source = readFileSync(resolve(root, 'vendor/retroarch/menu/drivers/rmlui.c'), 'utf8');

const failures = [];
function check(ok, what) {
  if (ok) {
    console.log(`  ok   ${what}`);
    return;
  }
  console.log(`  FAIL ${what}`);
  failures.push(what);
}

const signature = 'static void rib_focus_list(';
let start = -1;
for (let from = 0; ; ) {
  const at = source.indexOf(signature, from);
  if (at < 0)
    break;
  if (source.slice(at, at + 180).includes('{')) {
    start = at;
    break;
  }
  from = at + signature.length;
}
const end = start >= 0 ? source.indexOf('\nstatic ', start + signature.length) : -1;
const body = start >= 0 && end > start ? source.slice(start, end) : '';
const outside = body
  ? source.slice(0, start) + source.slice(end)
  : source;

check(
  source.includes('rib_focus_list(menu, rib_rmlui_hovered_list_row())'),
  'resting the pointer on a list row focuses that row',
);
check(
  body.length > 0 && !body.includes('disc'),
  'the shader list uses the same focus writer as every other list',
);
check(
  !outside.includes('menu->list_focus ='),
  'the keyboard does not keep a second writer of list_focus',
);

for (const design of ['native', 'disc']) {
  const css = readFileSync(
    resolve(root, 'integrations/designs', design, 'menu.rcss'),
    'utf8',
  );
  const hoverAt = css.indexOf('.list-row:hover');
  const hoverEnd = hoverAt >= 0 ? css.indexOf('}', hoverAt) : -1;
  const hover = hoverAt >= 0 && hoverEnd > hoverAt ? css.slice(hoverAt, hoverEnd) : '';
  check(
    hover.length > 0 && !hover.includes('border-color'),
    `${design}: a pointer on a list row paints its own outline`,
  );
}

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log('\none selection: the pointer and the keyboard share list_focus');
