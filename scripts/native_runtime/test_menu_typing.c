/* What we read from the keyboard as the menu pad, with the code in the fork:
 * input_driver_collect_system_input in input_driver.c, the function that the
 * runloop calls once a frame, called with one key held.
 *
 * While the menu is open, RetroArch reads a few keys as buttons of the menu
 * pad: Return is A, Backspace is B, the arrows are the d-pad. The menu text
 * entry receives the same keys as they are typed. A key typed into the menu
 * text entry is text and never also a button, so a Backspace typed into
 * QUICK SIGN IN does not leave the form.
 *
 * RetroArch is not started. We answer here the calls that the function makes
 * into the menu, and test_menu_typing_unreached.c stubs the rest of RetroArch. */
#include <stdio.h>
#include <string.h>
#include <libretro.h>
#include <retro_miscellaneous.h>
#include "configuration.h"
#include "input/input_driver.h"
#include "input/input_keymaps.h"
#include "menu/menu_driver.h"
#include "menu/menu_input.h"
#include "menu/drivers/rmlui_bridge.h"

static unsigned held;
static bool typing;
static struct menu_state menu;

/* The menu is open, with no on-screen keyboard showing. */
struct menu_state *menu_state_get_ptr(void) { return &menu; }
bool menu_input_dialog_get_display_kb(void) { return false; }
/* Whether the menu's text entry has the keyboard. */
bool rib_rmlui_typing(void) { return typing; }

static int16_t keyboard(void *data, const input_device_driver_t *joypad,
      const input_device_driver_t *sec_joypad, rarch_joypad_info_t *joypad_info,
      const retro_keybind_set *binds, bool keyboard_mapping_blocked,
      unsigned port, unsigned device, unsigned index, unsigned id)
{
   return device == RETRO_DEVICE_KEYBOARD && id == held;
}

static input_driver_t holding = { .input_state = keyboard, .ident = "holding" };

/* The menu's pad as the function reads it this frame, with `key` held. No
 * user is enabled, so we read no controller, only the keyboard. */
static input_bits_t read_with(unsigned key)
{
   static input_driver_state_t input;
   static settings_t settings;
   input_bits_t bits;
   input.current_driver = &holding;
   input.current_data = &holding;
   memset(&bits, 0, sizeof(bits));
   held = key;
   input_driver_collect_system_input(&input, &settings, &bits);
   return bits;
}

int main(void)
{
   int failures = 0;
   unsigned key;
   input_bits_t bits;
   menu.flags = MENU_ST_FLAG_ALIVE;

   /* We read the keyboard as in the menu: Backspace is B while nothing is
    * being typed. Without this case, the next one could pass by reading nothing. */
   typing = false;
   bits = read_with(RETROK_BACKSPACE);
   if (!BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_B))
   {
      fprintf(stderr, "FAIL: with nothing typed, Backspace is not the menu's B\n");
      ++failures;
   }

   /* While the text entry has the keyboard, no key is a button. */
   typing = true;
   for (key = RETROK_BACKSPACE; key < RETROK_LAST; ++key)
   {
      bits = read_with(key);
      if (bits_any_set(bits.data, ARRAY_SIZE(bits.data)))
      {
         char name[64] = "";
         input_keymaps_translate_rk_to_str((enum retro_key)key, name, sizeof(name));
         fprintf(stderr, "FAIL: typed into the menu's text entry, %s (key %u) is also a button of the menu's pad%s\n",
               *name ? name : "a key", key,
               BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_B) ? ": B, which leaves the form" : "");
         ++failures;
      }
   }
   if (!failures)
      printf("menu typing: a typed key is never the menu's pad; Backspace is B otherwise\n");
   return failures ? 1 : 0;
}
