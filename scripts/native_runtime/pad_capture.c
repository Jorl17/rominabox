/* The input we capture in the menu for a hotkey from a pad, with the code in
 * the fork: RetroArch's own capture (menu_input_rib_capture_start and
 * menu_input_rib_bind_poll in menu/menu_driver.c), started as for a hotkey
 * (rib_pad_input_capture_start in pad_inputs.c), and the pad input we read
 * the capture as through the pad's RetroArch profile.
 *
 *   pad_capture PROFILE FRAME...
 *
 * connects one pad with PROFILE, starts a capture with the pad as the first
 * FRAME has it, and polls the capture once for each later FRAME, with the
 * pad as that FRAME has it. A FRAME is a list of what is held, separated by
 * spaces: bN for button N, and aN:V for axis N at V (from -32768 to 32767,
 * 0 for any axis not listed). We print one line:
 *
 *   captured <the pad input we read the capture as, or none>
 *
 * or "captured nothing" when no FRAME completed the capture.
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
#include "msg_hash.h"
#include "input/input_driver.h"
#include "input/input_remapping.h"
#include "menu/menu_driver.h"
#include "menu/drivers/rmlui/pad_inputs.h"

enum { BUTTONS = 32, AXES = 8 };

static int held_buttons[BUTTONS];
static int held_axes[AXES];
static runloop_state_t runloop;
static retro_time_t now;

runloop_state_t *runloop_state_get_ptr(void) { return &runloop; }
retro_time_t cpu_features_get_time_usec(void) { return now; }
const char *msg_hash_to_str(enum msg_hash_enums msg)
{
   (void)msg;
   return "";
}

static int32_t button(unsigned pad, uint16_t joykey)
{
   return pad == 0 && joykey < BUTTONS && held_buttons[joykey];
}

/* As a RetroArch joypad driver reads an axis: one half of it at a time. */
static int16_t axis(unsigned pad, uint32_t joyaxis)
{
   if (pad != 0 || joyaxis == AXIS_NONE)
      return 0;
   if (AXIS_POS_GET(joyaxis) < AXES)
      return held_axes[AXIS_POS_GET(joyaxis)] > 0 ? held_axes[AXIS_POS_GET(joyaxis)] : 0;
   if (AXIS_NEG_GET(joyaxis) < AXES)
      return held_axes[AXIS_NEG_GET(joyaxis)] < 0 ? held_axes[AXIS_NEG_GET(joyaxis)] : 0;
   return 0;
}

static void poll(void) {}

static input_device_driver_t pad = { .button = button, .axis = axis, .poll = poll, .ident = "stand-in" };

/* No keyboard and no mouse. */
static int16_t nothing(void *data, const input_device_driver_t *joypad, const input_device_driver_t *sec_joypad,
      rarch_joypad_info_t *joypad_info, const retro_keybind_set *binds, bool keyboard_mapping_blocked,
      unsigned port, unsigned device, unsigned idx, unsigned id)
{
   (void)data, (void)joypad, (void)sec_joypad, (void)joypad_info, (void)binds;
   (void)keyboard_mapping_blocked, (void)port, (void)device, (void)idx, (void)id;
   return 0;
}

static input_driver_t keyboard = { .input_state = nothing, .ident = "stand-in" };

static void hold(const char *frame)
{
   char copy[256];
   char *item;
   memset(held_buttons, 0, sizeof held_buttons);
   memset(held_axes, 0, sizeof held_axes);
   snprintf(copy, sizeof copy, "%s", frame);
   for (item = strtok(copy, " "); item; item = strtok(NULL, " "))
   {
      unsigned index = (unsigned)atoi(item + 1);
      if (item[0] == 'b' && index < BUTTONS)
         held_buttons[index] = 1;
      else if (item[0] == 'a' && index < AXES && strchr(item, ':'))
         held_axes[index] = atoi(strchr(item, ':') + 1);
   }
}

int main(int argc, char **argv)
{
   config_file_t *profile;
   settings_t *settings;
   input_driver_state_t *input;
   struct retro_keybind captured;
   unsigned bind;
   int frame;

   if (argc < 3 || !(profile = config_file_new_from_path_to_string(argv[1])))
   {
      fprintf(stderr, "usage: pad_capture PROFILE FRAME...\n");
      return 2;
   }
   retroarch_config_init();
   settings = config_get_ptr();
   settings->uints.input_max_users = 1;
   settings->floats.input_axis_threshold = DEFAULT_AXIS_THRESHOLD;
   input_config_reset();
   settings->uints.input_joypad_index[0] = 0;
   input_config_reset_autoconfig_binds(0);
   input_config_set_autoconfig_binds(0, profile);
   config_file_free(profile);
   input = input_state_get_ptr();
   input->primary_joypad = &pad;
   input->current_driver = &keyboard;
   input->libretro_input_binds[0] = &input_config_binds[0];

   now = 1000000;
   hold(argv[2]);
   if (!rib_pad_input_capture_start(&captured, 5))
   {
      fprintf(stderr, "the capture did not start\n");
      return 1;
   }
   for (frame = 3; frame < argc; ++frame)
   {
      float remaining;
      now += 16000;
      hold(argv[frame]);
      if (menu_input_rib_bind_poll(now, &remaining, true) != MENU_RIB_BIND_CAPTURED)
         continue;
      if (rib_pad_input_of(captured.joykey, captured.joyaxis, &bind))
         printf("captured %s\n", rib_pad_input_id(bind));
      else
         printf("captured none\n");
      return 0;
   }
   printf("captured nothing\n");
   return 0;
}
