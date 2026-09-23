// When the player opens Controls, we focus the first control. When the pointer
// rests on another control, we open the list of that control and move the
// yellow highlight there too, so only one control looks selected.
//
// For the pause row we keep one index into the buttons in the document, and
// pressing a key moves that index. Controls have the same kind of index
// (control_focus). We set it from the pointer through the same function as
// for the keys, so the yellow highlight and the keyboard share one selection.
//
// rmlui.c is not linked into the bridge test because it depends on RetroArch,
// so here we compile the function itself, as in the pad-binding check.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const native = resolve(root, 'vendor/retroarch');
const output = resolve(root, 'work/review-control-focus');
mkdirSync(output, { recursive: true });

function extract(file, signature, nextSignature) {
  const source = readFileSync(resolve(native, file), 'utf8');
  const start = source.indexOf(signature);
  const end = source.indexOf(nextSignature, start);
  if (start < 0 || end < 0)
    throw new Error(`Missing function in ${file}: ${signature}`);
  return source.slice(start, end);
}

function extractOptional(file, signature, nextSignature) {
  const source = readFileSync(resolve(native, file), 'utf8');
  const start = source.indexOf(signature);
  if (start < 0)
    return '';
  const end = source.indexOf(nextSignature, start);
  if (end < 0)
    throw new Error(`Missing end for ${signature}`);
  return source.slice(start, end);
}

const active = extract('menu/drivers/rmlui/menu.cpp',
  'static bool rib_control_is_active(', '\nstatic const char *rib_control_console_name');
const step = extract('menu/drivers/rmlui/menu.cpp',
  'static int rib_control_step(', '\nstatic int rib_pause_row(');
const same = extract('menu/drivers/rmlui/menu.cpp',
  'static bool rib_same_bind_target(', '\nstatic void rib_bind_anchor(');
const focus = extractOptional('menu/drivers/rmlui/menu.cpp',
  'static void rib_focus_control(rib_rmlui_menu_t *menu, int index)\n',
  '\nstatic void rib_rmlui_update_binds(');
const update = extract('menu/drivers/rmlui/menu.cpp',
  'static void rib_rmlui_update_binds(', '\nvoid rib_menu_frame(');

const source = readFileSync(resolve(native, 'menu/drivers/rmlui/menu.cpp'), 'utf8');
const called = source.indexOf('rib_focus_control(menu, rib_control_step');
const assigned = source.indexOf('menu->control_focus = rib_control_step');
const focusWrite = called >= 0 ? called : assigned;
const keysAt = focusWrite >= 0 ? source.lastIndexOf('case RIB_KEY_UP:', focusWrite) : -1;
const keysEnd = focusWrite >= 0 ? source.indexOf('case RIB_KEY_OK:', focusWrite) : -1;
const keys = keysAt >= 0 && keysEnd > keysAt ? source.slice(keysAt, keysEnd) : '';

const generated = `
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>

#define RIB_CONTROL_MAX 48
#define RIB_RMLUI_ACTION_NONE 0
#define RIB_RMLUI_ACTION_CONTROLS_BACK 12
#define RIB_RMLUI_ACTION_CONTROLS_RESET 13
#define RIB_RMLUI_ACTION_CONTROL_FIRST 15
#define RIB_RMLUI_ACTION_CONTROL_LAST (RIB_RMLUI_ACTION_CONTROL_FIRST + 47)

typedef long long retro_time_t;

typedef struct rib_control {
   char id[32];
   char group[32];
   unsigned bind_index;
} rib_control_t;

typedef struct rib_rmlui_menu {
   bool controls_visible;
   bool capture_active;
   bool device_picker_open;
   int control_focus;
   int selected_control;
   rib_control_t controls[RIB_CONTROL_MAX];
   int control_count;
   bool control_active[RIB_CONTROL_MAX];
} rib_rmlui_menu_t;

static char rib_binds_list[64];
static int rib_binds_after_ms;
static int rib_binds_for = -1;
static retro_time_t rib_binds_since;
static bool rib_binds_open;

static int painted_focus = -999;
static int shown_for = -999;
static bool pointer_inside;
static int hovered_action;

static bool string_is_equal(const char *a, const char *b) {
   return a && b && strcmp(a, b) == 0;
}
static void rib_rmlui_refresh_controls(rib_rmlui_menu_t *menu) {
   painted_focus = menu ? menu->control_focus : -1;
}
static void rib_hide_binds(void) { rib_binds_open = false; }
static void rib_show_binds(rib_rmlui_menu_t *menu, int index) {
   (void)menu;
   shown_for = index;
   rib_binds_open = true;
}
static bool rib_rmlui_pointer_inside(const char *id, int x, int y) {
   (void)id; (void)x; (void)y;
   return pointer_inside;
}
static int rib_rmlui_hovered_action(void) { return hovered_action; }
static retro_time_t now_us;
static retro_time_t rib_host_time_us(void) { return now_us; }

${active}
${step}
${same}
${focus}
${update}

static const char *keyboard_cases = ${JSON.stringify(keys)};

static int failures;
static void check(bool ok, const char *what) {
   if (ok) { printf("  ok   %s\\n", what); return; }
   printf("  FAIL %s\\n", what);
   ++failures;
}

static void rest(rib_rmlui_menu_t *menu, int action) {
   hovered_action = action;
   pointer_inside = false;
   painted_focus = -999;
   rib_rmlui_update_binds(menu, 40, 40, true, true);
}

int main(void) {
   /* We use this when we set focus from the pointer, so the yellow is repainted. */
   (void)rib_rmlui_refresh_controls;
   rib_rmlui_menu_t menu;
   int index;
   memset(&menu, 0, sizeof(menu));
   menu.controls_visible = true;
   menu.control_count = 4;
   menu.control_focus = 0;
   for (index = 0; index < 4; ++index)
      menu.control_active[index] = true;
   strcpy(rib_binds_list, "control-binds");
   rib_binds_after_ms = 1200;
   rib_binds_for = -1;

   /* The pointer is on the third control. The yellow has to move there, and
    * the next key press has to move from there, not from the control that
    * was focused when the screen opened. */
   rest(&menu, RIB_RMLUI_ACTION_CONTROL_FIRST + 2);
   check(menu.control_focus == 2,
         "resting the pointer on a control focuses that control");
   check(painted_focus == 2,
         "the yellow is painted from that same focus");
   check(rib_control_step(&menu, menu.control_focus, 1) == 3,
         "the keyboard continues from the control under the pointer");

   /* Open the list the way the player does: the pointer rests, the delay
    * passes, the list is up. Then move onto Reset. The yellow leaves the
    * pad and the list closes. */
   menu.control_focus = 0;
   hovered_action = RIB_RMLUI_ACTION_CONTROL_FIRST;
   now_us = 0;
   rib_rmlui_update_binds(&menu, 40, 40, true, true);
   now_us = 2000000;
   rib_rmlui_update_binds(&menu, 40, 40, true, true);
   hovered_action = RIB_RMLUI_ACTION_CONTROLS_RESET;
   now_us = 4000000;
   painted_focus = -999;
   rib_rmlui_update_binds(&menu, 40, 40, true, true);
   check(menu.control_focus == RIB_CONTROL_MAX,
         "resting on Reset focuses Reset, not a control");
   check(!rib_binds_open, "resting on Reset closes the bind list");

   menu.control_focus = 1;
   pointer_inside = true;
   rib_binds_for = 1;
   rib_binds_open = true;
   hovered_action = RIB_RMLUI_ACTION_CONTROL_FIRST + 3;
   rib_rmlui_update_binds(&menu, 40, 40, true, true);
   check(menu.control_focus == 1,
         "the pointer over the open list keeps that control focused");

   hovered_action = RIB_RMLUI_ACTION_NONE;
   pointer_inside = false;
   rib_rmlui_update_binds(&menu, 1, 1, true, true);
   check(menu.control_focus == 1,
         "leaving the pointer does not drop the focus");

   check(strstr(keyboard_cases, "rib_focus_control(") != NULL,
         "the keyboard focuses through the same function as the pointer");
   check(strstr(keyboard_cases, "control_focus =") == NULL,
         "the keyboard does not keep a second writer of control_focus");

   if (failures) {
      printf("\\n%d control focus check(s) failed\\n", failures);
      return 1;
   }
   printf("\\none selection: the pointer and the keyboard share control_focus\\n");
   return 0;
}
`;

const file = resolve(output, 'control-focus.c');
const binary = resolve(output, 'control-focus');
writeFileSync(file, generated);
execFileSync('cc', ['-std=c11', '-Wall', '-Wextra', '-Werror', '-fsanitize=address',
  '-o', binary, file], { stdio: 'inherit' });
try {
  execFileSync(binary, { stdio: 'inherit' });
} catch (error) {
  process.exit(error.status ?? 1);
}
