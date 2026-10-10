/* How we compare two binds of player 1 on CONTROLS, and give the binds of
 * player 1 to every pad that plays as player 1, with the code in the fork
 * (pad_inputs.c), over the binds from the profile of a pad and the player's
 * own rebinds.
 *
 *   controls_binds PROFILE [--pads N] [--separate] [POSITION=VALUE...] -- LEFT RIGHT
 *
 * We connect N pads (1 when not given), each with PROFILE and each playing
 * as player 1, or each as a player of its own with --separate. For each
 * POSITION of the standard pad we give player 1 the pad input VALUE, in the
 * form of RetroArch's config ("13", "h0up", "+3"), as a rebind on CONTROLS
 * does, and then give the binds of player 1 to the other pads as after such
 * a rebind. We print one line for the positions LEFT and RIGHT:
 *
 *   conflict <yes or no>
 *
 * and then, for each port P from 1, the pad input of LEFT in the binds of
 * that port, which RetroArch reads for the pad there:
 *
 *   port <P> <13, h0up, +3 or none>
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
   unsigned left, right, port;
   unsigned pads = 1;
   int separate = 0;
   int index = 2;

   if (argc < 5 || strcmp(argv[argc - 3], "--")
         || !(profile = config_file_new_from_path_to_string(argv[1])))
   {
      fprintf(stderr, "usage: controls_binds PROFILE [--pads N] [--separate] [POSITION=VALUE...] -- LEFT RIGHT\n");
      return 2;
   }
   for (; index < argc - 3 && !strncmp(argv[index], "--", 2); ++index)
   {
      if (!strcmp(argv[index], "--separate"))
         separate = 1;
      else if (!strcmp(argv[index], "--pads") && index + 1 < argc - 3)
         pads = (unsigned)atoi(argv[++index]);
   }
   if (pads < 1 || pads > MAX_USERS)
      return 2;
   retroarch_config_init();
   settings = config_get_ptr();
   settings->uints.input_max_users = pads;
   input_config_reset();
   for (port = 0; port < MAX_USERS; ++port)
   {
      unsigned *mapped = settings->uints.input_remap_port_map[port];
      unsigned at;
      for (at = 0; at < MAX_USERS; ++at)
         mapped[at] = MAX_USERS;
      if (separate)
         mapped[0] = port < pads ? port : MAX_USERS;
      else if (port == 0)
         for (at = 0; at < pads; ++at)
            mapped[at] = at;
   }
   for (port = 0; port < pads; ++port)
   {
      settings->uints.input_joypad_index[port] = port;
      input_config_reset_autoconfig_binds(port);
      input_config_set_autoconfig_binds(port, profile);
   }
   config_file_free(profile);

   for (; index < argc - 3; ++index)
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
   rib_pad_input_share_player_one_binds();
   printf("conflict %s\n", rib_pad_input_binds_conflict(left, right) ? "yes" : "no");
   for (port = 0; port < pads; ++port)
   {
      char value[RIB_PAD_INPUT_VALUE_MAX];
      const struct retro_keybind *bound = &input_config_binds[port][left];
      if (rib_pad_input_value(bound->joykey, AXIS_NONE, value, sizeof(value))
            || rib_pad_input_value(NO_BTN, bound->joyaxis, value, sizeof(value)))
         printf("port %u %s\n", port + 1, value);
      else
         printf("port %u none\n", port + 1);
   }
   return 0;
}
