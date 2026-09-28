/* The input we read in the menu from a pad through its RetroArch profile,
 * with the code in the fork: pad_inputs.c, where we read Home and the
 * standard pad positions, over the binds set for a profile by the loader
 * (input_config_reset_autoconfig_binds and input_config_set_autoconfig_binds
 * in configuration.c) when that pad is connected.
 *
 *   pad_home PROFILE BUTTON
 *
 * loads PROFILE, presses BUTTON on a stand-in pad and keeps it down, and
 * prints one line for each of:
 *
 *   home <the button that is Home in the profile, or none>
 *   held <1 when we read Home as held in the menu, else 0>
 *   captured <the pad input that we read BUTTON as in the menu, or none>
 *
 * RetroArch does not start. retroarch_unreached.c stands in for the rest of
 * RetroArch. */
#include <stdio.h>
#include <stdlib.h>
#include <libretro.h>
#include <file/config_file.h>
#include "input/input_driver.h"
#include "input/input_remapping.h"
#include "menu/drivers/rmlui/pad_inputs.h"

static unsigned held_button;

static int32_t button(unsigned port, uint16_t joykey)
{
   return port == 0 && joykey == held_button;
}

static int16_t axis(unsigned port, uint32_t joyaxis)
{
   (void)port;
   (void)joyaxis;
   return 0;
}

static input_device_driver_t pad = { .button = button, .axis = axis, .ident = "stand-in" };

int main(int argc, char **argv)
{
   config_file_t *profile;
   unsigned home;
   unsigned captured;

   if (argc != 3 || !(profile = config_file_new_from_path_to_string(argv[1])))
   {
      fprintf(stderr, "usage: pad_home PROFILE BUTTON\n");
      return 2;
   }
   held_button = (unsigned)atoi(argv[2]);
   /* As when a pad is connected: clear every bind, then apply the profile. */
   input_config_reset_autoconfig_binds(0);
   input_config_set_autoconfig_binds(0, profile);
   config_file_free(profile);
   input_state_get_ptr()->primary_joypad = &pad;

   if (rib_pad_input_bind("home", &home) && input_autoconf_binds[0][home].joykey != NO_BTN)
      printf("home %u\n", (unsigned)input_autoconf_binds[0][home].joykey);
   else
      printf("home none\n");
   printf("held %d\n", rib_pad_input_bind("home", &home) && rib_pad_input_down(home));
   if (rib_pad_input_of((uint16_t)held_button, AXIS_NONE, &captured))
      printf("captured %s\n", rib_pad_input_id(captured));
   else
      printf("captured none\n");
   return 0;
}
