/* What we read from a controller as the menu pad, with the code in the
 * fork: input_driver_collect_system_input in input_driver.c, the function
 * that the runloop calls once a frame, called with one controller button held.
 *
 * The profile of a controller lists which of its buttons is each button of
 * the standard pad (input_autoconf_binds). The game controls, which the
 * player changes on CONTROLS, move a console button to another controller
 * button (input_config_binds). Stock RetroArch reads the menu pad through the
 * game controls. In the ROM-in-a-Box menu we read a controller through its
 * profile, whatever the game controls, so Left on the d-pad is Left there.
 *
 * RetroArch is not started. We answer here the calls that the function makes
 * into the menu, and retroarch_unreached.c stubs the rest of RetroArch. */
#include <stdio.h>
#include <string.h>
#include <libretro.h>
#include <retro_miscellaneous.h>
#include "configuration.h"
#include "input/input_driver.h"
#include "menu/menu_driver.h"
#include "menu/menu_input.h"
#include "menu/drivers/rmlui_bridge.h"

/* The controller buttons: Left on the d-pad, as listed in its profile, and
 * the button that the game controls move Left to. */
#define DPAD_LEFT 14
#define MOVED_LEFT 2

static uint16_t held;
static struct menu_state menu;
static settings_t settings;

/* The menu is open, with no on-screen keyboard showing and nothing typed. */
struct menu_state *menu_state_get_ptr(void) { return &menu; }
bool menu_input_dialog_get_display_kb(void) { return false; }
bool rib_rmlui_typing(void) { return false; }
bool rib_rmlui_menu_hotkey_key(unsigned code) { (void)code; return false; }
/* Whether ROM-in-a-Box's menu is open. */
bool rib_rmlui_reads_input(void) { return menu.flags & MENU_ST_FLAG_ALIVE; }
settings_t *config_get_ptr(void) { return &settings; }

/* A controller as reported by the joypad drivers of the platforms
 * (sdl_joypad.c, dinput_joypad.c): a button of the standard pad is held when
 * the controller button in its bind is held, from the game controls or, where
 * they have none, from the profile. */
static int32_t pad_button(unsigned port, uint16_t joykey)
{
   (void)port;
   return joykey != NO_BTN && joykey == held;
}

static int16_t pad_state(rarch_joypad_info_t *joypad_info, const struct retro_keybind *binds, unsigned port)
{
   int16_t buttons = 0;
   unsigned i;
   for (i = 0; i < RARCH_FIRST_CUSTOM_BIND; i++)
   {
      uint16_t joykey = binds[i].joykey != NO_BTN ? binds[i].joykey : joypad_info->auto_binds[i].joykey;
      if (pad_button(port, joykey))
         buttons |= 1 << i;
   }
   return buttons;
}

static int16_t pad_axis(unsigned port, uint32_t joyaxis) { (void)port; (void)joyaxis; return 0; }
static bool pad_present(unsigned port) { return port == 0; }

static input_device_driver_t pad = {
   .query_pad = pad_present, .button = pad_button, .state = pad_state, .axis = pad_axis, .ident = "holding" };

/* No keyboard key is held. */
static int16_t no_key(void *data, const input_device_driver_t *joypad,
      const input_device_driver_t *sec_joypad, rarch_joypad_info_t *joypad_info,
      const retro_keybind_set *binds, bool keyboard_mapping_blocked,
      unsigned port, unsigned device, unsigned index, unsigned id)
{
   (void)data; (void)joypad; (void)sec_joypad; (void)joypad_info; (void)binds;
   (void)keyboard_mapping_blocked; (void)port; (void)device; (void)index; (void)id;
   return 0;
}

static input_driver_t keyboard = { .input_state = no_key, .ident = "none" };

/* The menu's pad as the function reads it this frame, with the controller's
 * `button` held. */
static input_bits_t read_with(uint16_t button)
{
   input_driver_state_t *input = input_state_get_ptr();
   input_bits_t bits;
   input->current_driver = &keyboard;
   input->current_data = &keyboard;
   input->primary_joypad = &pad;
   /* As set by RetroArch when it starts (input_driver.c). */
   input->libretro_input_binds[0] = (const retro_keybind_set *)&input_config_binds[0];
   memset(input->joypad_state_cache_valid, 0, sizeof(input->joypad_state_cache_valid));
   memset(&bits, 0, sizeof(bits));
   held = button;
   input_driver_collect_system_input(input, &settings, &bits);
   return bits;
}

int main(void)
{
   int failures = 0;
   unsigned i;
   input_bits_t bits;

   /* One user with a controller whose profile lists only Left on the d-pad,
    * and whose game controls move Left to another button. */
   settings.uints.input_max_users = 1;
   settings.uints.input_libretro_device[0] = RETRO_DEVICE_JOYPAD;
   for (i = 0; i < RARCH_BIND_LIST_END; i++)
   {
      input_config_binds[0][i].joykey = NO_BTN;
      input_config_binds[0][i].joyaxis = AXIS_NONE;
      input_autoconf_binds[0][i].joykey = NO_BTN;
      input_autoconf_binds[0][i].joyaxis = AXIS_NONE;
   }
   input_autoconf_binds[0][RETRO_DEVICE_ID_JOYPAD_LEFT].joykey = DPAD_LEFT;
   input_config_binds[0][RETRO_DEVICE_ID_JOYPAD_LEFT].joykey = MOVED_LEFT;

   /* While the game plays, the button that Left was moved to is Left. Without
    * this case, the cases after it could pass by reading nothing. */
   bits = read_with(MOVED_LEFT);
   if (!BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_LEFT))
   {
      fprintf(stderr, "FAIL: while the game plays, the button the game's Left was moved to is not Left\n");
      ++failures;
   }

   menu.flags = MENU_ST_FLAG_ALIVE;
   /* In the menu, Left on the d-pad is Left. */
   bits = read_with(DPAD_LEFT);
   if (!BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_LEFT))
   {
      fprintf(stderr, "FAIL: in the menu, the d-pad's Left, which the game's controls moved, is not the menu's Left\n");
      ++failures;
   }
   /* And the button that the game's Left was moved to is no menu button. */
   bits = read_with(MOVED_LEFT);
   if (bits_any_set(bits.data, ARRAY_SIZE(bits.data)))
   {
      fprintf(stderr, "FAIL: in the menu, the button the game's Left was moved to is a button of the menu's pad%s\n",
            BIT256_GET(bits, RETRO_DEVICE_ID_JOYPAD_LEFT) ? ": Left" : "");
      ++failures;
   }
   if (!failures)
      printf("menu pad: in the menu a controller navigates by its profile; the game's controls move the game's buttons alone\n");
   return failures ? 1 : 0;
}
