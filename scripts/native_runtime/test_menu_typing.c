/* What we read from the keyboard as the menu pad, with the code in the fork:
 * input_driver_collect_system_input in input_driver.c, the function that the
 * runloop calls once a frame, called with one key held.
 *
 * While the menu is open, stock RetroArch reads keys as buttons of the menu
 * pad: a few fixed keys (Return is A, Backspace is B, Space is Start, Right
 * Shift is Select, the arrows are the d-pad) and every key bound in the game
 * controls (the game's Start, Enter by default, is Start). In the ROM-in-a-Box
 * menu we read only the HOTKEYS keys and the arrows from the keyboard, so
 * neither the game's Start moved to P nor Space saves on the pause screen.
 *
 * A key typed into the menu text entry is text and never also a button, so
 * a Backspace typed into QUICK SIGN IN does not leave the form. A key bound
 * to a hotkey that acts in the menu is only that hotkey, even an arrow.
 *
 * RetroArch is not started. We answer here the calls that the function makes
 * into the menu, and retroarch_unreached.c stubs the rest of RetroArch. */
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
static unsigned bound;
static struct menu_state menu;
static settings_t settings;

/* The menu is open, with no on-screen keyboard showing. */
struct menu_state *menu_state_get_ptr(void) { return &menu; }
bool menu_input_dialog_get_display_kb(void) { return false; }
/* Whether the menu's text entry has the keyboard. */
bool rib_rmlui_typing(void) { return typing; }
/* Whether a hotkey that acts in the menu is bound to the key `code`. */
bool rib_rmlui_menu_hotkey_key(unsigned code) { return code == bound; }
/* Whether ROM-in-a-Box's menu is open. */
bool rib_rmlui_reads_keyboard(void) { return menu.flags & MENU_ST_FLAG_ALIVE; }
/* The game's controller, which the function requests while the game plays. */
settings_t *config_get_ptr(void) { return &settings; }

/* A keyboard as reported by the keyboard drivers of the platforms (dinput.c,
 * cocoa_input.m): the held key, and the pad buttons whose binds contain it,
 * unless keyboard mapping is blocked. */
static int16_t keyboard(void *data, const input_device_driver_t *joypad,
      const input_device_driver_t *sec_joypad, rarch_joypad_info_t *joypad_info,
      const retro_keybind_set *binds, bool keyboard_mapping_blocked,
      unsigned port, unsigned device, unsigned index, unsigned id)
{
   int16_t buttons = 0;
   unsigned i;
   if (device == RETRO_DEVICE_KEYBOARD)
      return id == held;
   if (device != RETRO_DEVICE_JOYPAD || id != RETRO_DEVICE_ID_JOYPAD_MASK || keyboard_mapping_blocked)
      return 0;
   for (i = 0; i < RARCH_FIRST_CUSTOM_BIND; i++)
      if (binds[port][i].valid && binds[port][i].key == held)
         buttons |= 1 << i;
   return buttons;
}

static input_driver_t holding = { .input_state = keyboard, .ident = "holding" };

/* The menu's pad as the function reads it this frame, with `key` held: the
 * keyboard, and the game's controls on it for the one user. */
static input_bits_t read_with(unsigned key)
{
   /* The RetroArch input state, from which we take the driver when we read
    * the controls of a user. */
   input_driver_state_t *input = input_state_get_ptr();
   input_bits_t bits;
   input->current_driver = &holding;
   input->current_data = &holding;
   memset(&bits, 0, sizeof(bits));
   held = key;
   input_driver_collect_system_input(input, &settings, &bits);
   return bits;
}

int main(void)
{
   int failures = 0;
   unsigned key;
   size_t i;
   input_bits_t bits;
   /* The fixed RetroArch keys for the menu pad, besides the arrows. */
   static const unsigned own[] = { RETROK_RETURN, RETROK_BACKSPACE, RETROK_DELETE, RETROK_SLASH,
      RETROK_SPACE, RETROK_RSHIFT, RETROK_PAGEUP, RETROK_PAGEDOWN, RETROK_HOME, RETROK_END };
   typing = false;

   /* One user, a controller, whose game's Start is P. */
   settings.uints.input_max_users = 1;
   settings.uints.input_libretro_device[0] = RETRO_DEVICE_JOYPAD;
   input_config_binds[0][RETRO_DEVICE_ID_JOYPAD_START].valid = true;
   input_config_binds[0][RETRO_DEVICE_ID_JOYPAD_START].key = RETROK_p;

   /* While the game plays, P is its Start. Without this the case after it
    * could pass by reading nothing. */
   bits = read_with(RETROK_p);
   if (!BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_START))
   {
      fprintf(stderr, "FAIL: while the game plays, P, the game's Start, is not Start\n");
      ++failures;
   }

   menu.flags = MENU_ST_FLAG_ALIVE;
   /* In the menu P is the game's, not the menu's. */
   bits = read_with(RETROK_p);
   if (bits_any_set(bits.data, ARRAY_SIZE(bits.data)))
   {
      fprintf(stderr, "FAIL: in the menu, P, the game's Start, is a button of the menu's pad%s\n",
            BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_START) ? ": Start, which saves on the pause screen" : "");
      ++failures;
   }

   /* The arrows move in the menu. Without this the cases after it could pass
    * by reading nothing. */
   bits = read_with(RETROK_UP);
   if (!BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_UP))
   {
      fprintf(stderr, "FAIL: in the menu, Up is not the menu's Up\n");
      ++failures;
   }

   /* The other RetroArch keys for the menu pad are no button. */
   for (i = 0; i < ARRAY_SIZE(own); i++)
   {
      bits = read_with(own[i]);
      if (bits_any_set(bits.data, ARRAY_SIZE(bits.data)))
      {
         char name[64] = "";
         input_keymaps_translate_rk_to_str((enum retro_key)own[i], name, sizeof(name));
         fprintf(stderr, "FAIL: in the menu, %s is a button of the menu's pad%s\n", name,
               BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_START) ? ": Start, which saves on the pause screen" : "");
         ++failures;
      }
   }

   /* An arrow a hotkey of the menu is bound to is that hotkey alone. */
   bound = RETROK_UP;
   bits = read_with(RETROK_UP);
   if (bits_any_set(bits.data, ARRAY_SIZE(bits.data)))
   {
      fprintf(stderr, "FAIL: Up, bound to a hotkey of the menu, is also a button of the menu's pad\n");
      ++failures;
   }
   bound = 0;

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
      printf("menu typing: in the menu the keyboard's pad is the arrows alone; a typed key, a hotkey's and the game's are never its buttons\n");
   return failures ? 1 : 0;
}
