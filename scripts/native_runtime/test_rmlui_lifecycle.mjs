// Compile the cleanup and capture functions in isolation. We stub only the
// renderer and device-input boundaries, and do not launch the emulator.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const native = resolve(root, 'vendor/retroarch');
const output = resolve(root, 'work/review-lifecycle');
mkdirSync(output, { recursive: true });
function extract(file, signature, nextSignature) {
  const source = readFileSync(resolve(native, file), 'utf8');
  const start = source.indexOf(signature);
  const end = source.indexOf(nextSignature, start);
  if (start < 0 || end < 0) throw new Error(`Missing function in ${file}`);
  return source.slice(start, end);
}
const cleanup = extract('menu/drivers/rmlui.c',
  'static void rib_rmlui_free(void *data)', '\nstatic void rib_rmlui_context_destroy');
const capture = extract('menu/menu_driver.c',
  'enum menu_rib_bind_result menu_input_rib_bind_poll(', '\nvoid menu_input_rib_bind_cancel');
const source = `
#include <stdlib.h>
#include <stdbool.h>
#include <stdint.h>
#include <assert.h>
typedef struct { bool capture_active; } rib_rmlui_menu_t;
static void *rib_rmlui_active_menu;
static bool rib_splash_active;
static void rib_rmlui_cancel_capture(void *p, void *q) {}
static void rib_rmlui_shutdown(void) {}
${cleanup}
typedef int64_t retro_time_t;
typedef struct { struct { unsigned input_bind_timeout, input_bind_hold; } uints; } settings_t;
struct menu_bind_state { struct { retro_time_t timeout_end; } timer_timeout; };
struct menu_state { unsigned flags; struct menu_bind_state input_binds; };
typedef struct { char *s; unsigned len; } menu_input_ctx_bind_t;
enum menu_rib_bind_result { MENU_RIB_BIND_ACTIVE, MENU_RIB_BIND_CAPTURED, MENU_RIB_BIND_TIMED_OUT };
#define MENU_LABEL_MAX_LENGTH 256
#define MENU_ST_FLAG_IS_BINDING 1
static settings_t settings;
static struct menu_state menu_driver_state;
static unsigned polls, cancellations;
static settings_t *config_get_ptr(void) { return &settings; }
static bool menu_input_key_bind_iterate(settings_t *s, menu_input_ctx_bind_t *b, retro_time_t t) { ++polls; return false; }
static void menu_input_rib_bind_cancel(void) { ++cancellations; }
${capture}
int main(void) {
  // RetroArch calls cleanup, then frees its userdata.
  void *state = calloc(1, sizeof(rib_rmlui_menu_t));
  rib_rmlui_active_menu = state;
  rib_rmlui_free(state);
  free(state);
  float remaining = -1;
  menu_driver_state.input_binds.timer_timeout.timeout_end = 10000000;
  assert(menu_input_rib_bind_poll(5000000, &remaining, false) == MENU_RIB_BIND_ACTIVE);
  assert(remaining == 5 && polls == 0);
  assert(menu_input_rib_bind_poll(6000000, &remaining, false) == MENU_RIB_BIND_ACTIVE);
  assert(remaining == 4 && polls == 0);
  assert(menu_input_rib_bind_poll(10000000, &remaining, false) == MENU_RIB_BIND_TIMED_OUT);
  assert(remaining == 0 && cancellations == 1);
  assert(menu_input_rib_bind_poll(5000000, &remaining, true) == MENU_RIB_BIND_ACTIVE);
  assert(polls == 1);
  return 0;
}`;
const fixture = resolve(output, 'lifecycle.c');
const executable = resolve(output, 'lifecycle');
writeFileSync(fixture, source);
execFileSync('cc', ['-fsanitize=address', '-g', fixture, '-o', executable], { stdio: 'inherit' });
execFileSync(executable, [], { stdio: 'inherit' });
console.log('cleanup ownership and capture deadlines: ok');
