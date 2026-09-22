// The binding we show for a control on the controls screen, compiled from
// the function itself.
//
// RetroArch resolves a press from TWO arrays: the binds in the configuration,
// and the binds from the autoconfig profile of the pad that is plugged in. In
// the input path, RetroArch takes the explicit bind when there is one and the
// autoconfigured one otherwise, per field.
//
// On the controls screen we read both arrays the same way, so for a pad bound
// only by autoconfig, such as a DualSense, we show its buttons and not only
// the keyboard key.
//
// We extract rib_lines_from_bind and the helper next to it from
// vendor/retroarch and run them against binds that we construct in the test.
// We stub only the string formatting, and the choice of the bind in effect is
// the production code.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const native = resolve(root, 'vendor/retroarch');
const output = resolve(root, 'work/review-pad-bindings');
mkdirSync(output, { recursive: true });

function extract(file, signature, nextSignature) {
  const source = readFileSync(resolve(native, file), 'utf8');
  const start = source.indexOf(signature);
  const end = source.indexOf(nextSignature, start);
  if (start < 0 || end < 0) throw new Error(`Missing function in ${file}: ${signature}`);
  return source.slice(start, end);
}

const push = extract('menu/drivers/rmlui.c',
  'static void rib_push_bind_line(', '\nstatic const struct retro_keybind *rib_effective_pad');
const lines = extract('menu/drivers/rmlui.c',
  'static const struct retro_keybind *rib_effective_pad(', '\nstatic bool rib_same_bind_target');

const source = `
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <assert.h>

#define NO_BTN 0xffff
#define AXIS_NONE 0xffffffff
#define RIB_BIND_LINE_MAX 64
#define MAX_USERS 16
#define RARCH_BIND_LIST_END 256

struct retro_keybind {
   unsigned key;
   uint16_t joykey;
   uint32_t joyaxis;
   uint16_t mbutton;
   char *joykey_label;
   char *joyaxis_label;
};
typedef struct retro_keybind retro_keybind_set[RARCH_BIND_LIST_END];
retro_keybind_set input_autoconf_binds[MAX_USERS];
retro_keybind_set input_config_binds[MAX_USERS];

/* strlcpy comes from the platform here, and from RetroArch in the player build. */

/* We stub only the formatting. The choice of bind is the production code. */
static void input_keymaps_translate_rk_to_str(unsigned key, char *out, size_t len) {
   if (key) snprintf(out, len, "key%u", key); else strlcpy(out, "nul", len);
}
static size_t input_config_get_bind_string_joykey(bool show, char *out,
      const char *prefix, const struct retro_keybind *bind, size_t len) {
   (void)show; (void)prefix;
   snprintf(out, len, "btn%u", (unsigned)bind->joykey);
   return strlen(out);
}
static size_t input_config_get_bind_string_joyaxis(bool show, char *out,
      const char *prefix, const struct retro_keybind *bind, size_t len) {
   (void)show; (void)prefix;
   snprintf(out, len, "axis%u", (unsigned)bind->joyaxis);
   return strlen(out);
}
static void rib_mouse_label(uint16_t button, char *out, size_t length) {
   (void)button; strlcpy(out, "Left", length);
}

${push}
${lines}

static int find(char kinds[][8], int count, const char *kind) {
   int index;
   for (index = 0; index < count; ++index)
      if (strcmp(kinds[index], kind) == 0)
         return index;
   return -1;
}

static int failures;
static void check(bool ok, const char *what) {
   if (ok) { printf("  ok   %s\\n", what); return; }
   printf("  FAIL %s\\n", what);
   ++failures;
}

int main(void) {
   char details[RIB_BIND_LINE_MAX][64];
   char kinds[RIB_BIND_LINE_MAX][8];
   const unsigned at = 7;
   int count;

   /* A pad the player plugged in, bound by an autoconfig profile, with no
    * explicit bind, such as a DualSense. */
   memset(input_config_binds, 0, sizeof(input_config_binds));
   memset(input_autoconf_binds, 0, sizeof(input_autoconf_binds));
   input_config_binds[0][at].key     = 42;
   input_config_binds[0][at].joykey  = NO_BTN;
   input_config_binds[0][at].joyaxis = AXIS_NONE;
   input_config_binds[0][at].mbutton = NO_BTN;
   input_autoconf_binds[0][at].joykey  = 3;
   input_autoconf_binds[0][at].joyaxis = AXIS_NONE;

   count = 0;
   rib_lines_from_bind(&input_config_binds[0][at], at, details, kinds, &count);
   check(find(kinds, count, "KEY") >= 0, "the keyboard key is listed");
   check(find(kinds, count, "PAD") >= 0,
         "the pad button autoconfig bound is listed too");
   if (find(kinds, count, "PAD") >= 0)
      check(strcmp(details[find(kinds, count, "PAD")], "btn3") == 0,
            "and it is the button the profile gave");

   /* An explicit bind takes precedence over the autoconfigured one, the same
    * rule as in the input path when we check whether a press happened. */
   input_config_binds[0][at].joykey = 9;
   count = 0;
   rib_lines_from_bind(&input_config_binds[0][at], at, details, kinds, &count);
   check(find(kinds, count, "PAD") >= 0
         && strcmp(details[find(kinds, count, "PAD")], "btn9") == 0,
         "an explicit pad bind wins over the autoconfigured one");

   /* Per field, not per bind: we show a stick from autoconfig even when the
    * button next to it was bound by hand. */
   input_config_binds[0][at].joyaxis  = AXIS_NONE;
   input_autoconf_binds[0][at].joyaxis = 5;
   count = 0;
   rib_lines_from_bind(&input_config_binds[0][at], at, details, kinds, &count);
   check(find(kinds, count, "PAD") >= 0
         && strcmp(details[find(kinds, count, "PAD")], "btn9") == 0,
         "the explicit button is still the one shown");
   check(find(kinds, count, "AXIS") >= 0
         && strcmp(details[find(kinds, count, "AXIS")], "axis5") == 0,
         "and the autoconfigured axis is shown beside it");

   /* Nothing plugged in, nothing configured: one line, the keyboard's. */
   memset(input_autoconf_binds, 0, sizeof(input_autoconf_binds));
   input_autoconf_binds[0][at].joykey  = NO_BTN;
   input_autoconf_binds[0][at].joyaxis = AXIS_NONE;
   input_config_binds[0][at].joykey    = NO_BTN;
   input_config_binds[0][at].joyaxis   = AXIS_NONE;
   count = 0;
   rib_lines_from_bind(&input_config_binds[0][at], at, details, kinds, &count);
   check(count == 1 && strcmp(kinds[0], "KEY") == 0,
         "with no pad at all there is one line and it is the keyboard's");

   if (failures) {
      printf("\\n%d pad binding check(s) failed\\n", failures);
      return 1;
   }
   printf("\\nthe controls screen lists every input a press can come from\\n");
   return 0;
}
`;

const file = resolve(output, 'pad-bindings.c');
const binary = resolve(output, 'pad-bindings');
writeFileSync(file, source);
execFileSync('cc', ['-std=c11', '-Wall', '-Wextra', '-Werror', '-fsanitize=address',
  '-o', binary, file], { stdio: 'inherit' });
execFileSync(binary, { stdio: 'inherit' });
