/* How we compare two binds of player 1 on CONTROLS, with the code in the fork
 * (pad_inputs.c), over the binds from the profile of a pad and the player's
 * own rebinds.
 *
 *   controls_binds PROFILE [POSITION=VALUE...] -- LEFT RIGHT
 *
 * We connect one pad with PROFILE, and for each POSITION of the standard pad
 * give player 1 the pad input VALUE, in the form of RetroArch's config ("13",
 * "h0up", "+3"), as a rebind on CONTROLS does. We print one line for the
 * positions LEFT and RIGHT:
 *
 *   conflict <yes or no>
 *
 * We do not start RetroArch, and link retroarch_unreached.c in place of the
 * rest of it. */
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

static runloop_state_t runloop;

runloop_state_t *runloop_state_get_ptr(void) { return &runloop; }

int main(int argc, char **argv)
{
   config_file_t *profile;
   settings_t *settings;
   unsigned left, right;
   int index;

   if (argc < 5 || strcmp(argv[argc - 3], "--")
         || !(profile = config_file_new_from_path_to_string(argv[1])))
   {
      fprintf(stderr, "usage: controls_binds PROFILE [POSITION=VALUE...] -- LEFT RIGHT\n");
      return 2;
   }
   retroarch_config_init();
   settings = config_get_ptr();
   settings->uints.input_max_users = 1;
   input_config_reset();
   settings->uints.input_joypad_index[0] = 0;
   input_config_reset_autoconfig_binds(0);
   input_config_set_autoconfig_binds(0, profile);
   config_file_free(profile);

   for (index = 2; index < argc - 3; ++index)
   {
      char position[32];
      const char *value = strchr(argv[index], '=');
      unsigned bind;
      struct retro_keybind *bound;
      if (!value || (size_t)(value - argv[index]) >= sizeof(position))
         return 2;
      memcpy(position, argv[index], (size_t)(value - argv[index]));
      position[value - argv[index]] = '\0';
      if (!rib_pad_input_bind(position, &bind))
      {
         fprintf(stderr, "%s is no position of the standard pad\n", position);
         return 2;
      }
      bound = &input_config_binds[0][bind];
      if (!rib_pad_input_parse(value + 1, &bound->joykey, &bound->joyaxis))
      {
         fprintf(stderr, "%s is no pad input\n", value + 1);
         return 2;
      }
   }
   if (!rib_pad_input_bind(argv[argc - 2], &left) || !rib_pad_input_bind(argv[argc - 1], &right))
      return 2;
   printf("conflict %s\n", rib_pad_input_binds_conflict(left, right) ? "yes" : "no");
   return 0;
}
