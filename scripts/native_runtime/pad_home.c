/* The input we read in the menu from a pad through its RetroArch profile,
 * with the code in the fork: pad_inputs.c, where we read Home and the
 * standard pad positions, over the binds set for a profile by the loader
 * (input_config_reset_autoconfig_binds and input_config_set_autoconfig_binds
 * in configuration.c) when that pad is connected, and the ports that play as
 * player 1 after the remap loader (input_remapping_load_file) runs.
 *
 *   pad_home PROFILE BUTTON [PAD REMAP]
 *
 * connects as many pads as RetroArch reads by default (input_max_users),
 * each at a separate port and each with PROFILE, loads the exported REMAP
 * (or none), presses BUTTON on pad PAD (counted from 1, or the first when
 * not given) and keeps it down, and prints one line for each of:
 *
 *   home <the button that is Home in the profile, or none>
 *   held <1 when we read Home as held in the menu, else 0>
 *   captured <the pad input that we read BUTTON as in the menu, or none>
 *
 * RetroArch does not start. retroarch_unreached.c stands in for the rest of
 * RetroArch. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libretro.h>
#include <file/config_file.h>
#include "configuration.h"
#include "config.def.h"
#include "runloop.h"
#include "input/input_driver.h"
#include "input/input_remapping.h"
#include "menu/drivers/rmlui/pad_inputs.h"

enum { PADS = DEFAULT_INPUT_MAX_USERS };

static unsigned held_pad;
static unsigned held_button;
static runloop_state_t runloop;

runloop_state_t *runloop_state_get_ptr(void) { return &runloop; }

static int32_t button(unsigned pad, uint16_t joykey)
{
   return pad == held_pad && joykey == held_button;
}

static int16_t axis(unsigned pad, uint32_t joyaxis)
{
   (void)pad;
   (void)joyaxis;
   return 0;
}

static input_device_driver_t pad = { .button = button, .axis = axis, .ident = "stand-in" };

int main(int argc, char **argv)
{
   config_file_t *profile;
   settings_t *settings;
   unsigned home;
   unsigned captured;
   unsigned port;

   if ((argc != 3 && argc != 5) || !(profile = config_file_new_from_path_to_string(argv[1])))
   {
      fprintf(stderr, "usage: pad_home PROFILE BUTTON [PAD REMAP]\n");
      return 2;
   }
   held_button = (unsigned)atoi(argv[2]);
   held_pad = argc == 5 ? (unsigned)atoi(argv[3]) - 1 : 0;
   if (held_pad >= PADS)
   {
      fprintf(stderr, "no pad %s\n", argv[3]);
      return 2;
   }

   /* The RetroArch defaults for this: one pad at each port, as many ports
    * as RetroArch reads, and no remap. */
   retroarch_config_init();
   settings = config_get_ptr();
   settings->uints.input_max_users = DEFAULT_INPUT_MAX_USERS;
   settings->floats.input_axis_threshold = DEFAULT_AXIS_THRESHOLD;
   input_config_reset();
   input_remapping_set_defaults(true);
   for (port = 0; port < PADS; ++port)
   {
      settings->uints.input_joypad_index[port] = port;
      /* As when a pad is connected: clear every bind, then apply the profile. */
      input_config_reset_autoconfig_binds(port);
      input_config_set_autoconfig_binds(port, profile);
   }
   config_file_free(profile);
   if (argc == 5 && strcmp(argv[4], "none"))
   {
      config_file_t *remap = config_file_new_from_path_to_string(argv[4]);
      if (!remap || !input_remapping_load_file(remap, argv[4]))
      {
         fprintf(stderr, "cannot load the remap %s\n", argv[4]);
         return 2;
      }
      config_file_free(remap);
   }
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
