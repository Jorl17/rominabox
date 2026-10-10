/* The input we capture in the menu from a pad, with the code in the fork:
 * RetroArch's own capture (menu_input_rib_bind_poll in menu/menu_driver.c),
 * started as for a hotkey or for a control on CONTROLS (pad_inputs.c), and
 * the pad input we read in the capture through the profile of the pad we
 * captured from.
 *
 *   pad_capture [--controls POSITION] [--pads N] [--named] PROFILE FRAME...
 *
 * We connect N pads (1 when not given), each with PROFILE and each playing
 * as player 1, start a capture with the pads as in the first FRAME, and poll
 * the capture once for each later FRAME, with the pads as in that FRAME.
 * Without --controls the capture is for a hotkey. With it, the capture is
 * for the control of CONTROLS on POSITION, a position of the standard pad.
 * In a FRAME we list what the player holds, separated by spaces: bN for
 * button N and aN:V for axis N at V (from -32768 to 32767, 0 for any axis not
 * listed), on the first pad, or on pad P (from 1) as P:bN and P:aN:V. We
 * print one line:
 *
 *   captured <input> on pad <P>
 *
 * where <input> is the position of the standard pad, or home, of the input
 * captured in the profile of the pad, or else the input itself, as
 * "button N" or "axis +N" or "axis -N". When we capture nothing in any FRAME,
 * we print "captured nothing". With --named we also print the input in the
 * form of RetroArch's config, and its name, as we find them in the host for
 * the menu (host.c, rib_host_pad_value_input and rib_host_pad_name):
 *
 *   value <13, h0up or +3>
 *   named <the name of its position in the profile of the pad we captured from, or none>
 *   read <the input we read for it in a binding to a hotkey, as above>
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
#include "msg_hash.h"
#include "input/input_driver.h"
#include "input/input_remapping.h"
#include "menu/menu_driver.h"
#include "menu/drivers/rmlui/pad_inputs.h"

enum { PADS = 4, BUTTONS = 32, AXES = 8 };

static int held_buttons[PADS][BUTTONS];
static int held_axes[PADS][AXES];
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
   return pad < PADS && joykey < BUTTONS && held_buttons[pad][joykey];
}

/* As a RetroArch joypad driver reads an axis: one half of it at a time. */
static int16_t axis(unsigned pad, uint32_t joyaxis)
{
   if (pad >= PADS || joyaxis == AXIS_NONE)
      return 0;
   if (AXIS_POS_GET(joyaxis) < AXES)
      return held_axes[pad][AXIS_POS_GET(joyaxis)] > 0 ? held_axes[pad][AXIS_POS_GET(joyaxis)] : 0;
   if (AXIS_NEG_GET(joyaxis) < AXES)
      return held_axes[pad][AXIS_NEG_GET(joyaxis)] < 0 ? held_axes[pad][AXIS_NEG_GET(joyaxis)] : 0;
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
      unsigned on = 0;
      unsigned index;
      if (strchr(item, ':') && item[0] >= '1' && item[0] <= '9' && item[1] == ':')
      {
         on = (unsigned)(item[0] - '1');
         item += 2;
      }
      index = (unsigned)atoi(item + 1);
      if (on >= PADS)
         continue;
      if (item[0] == 'b' && index < BUTTONS)
         held_buttons[on][index] = 1;
      else if (item[0] == 'a' && index < AXES && strchr(item, ':'))
         held_axes[on][index] = atoi(strchr(item, ':') + 1);
   }
}

static void print_named(const struct retro_keybind *captured)
{
   char value[RIB_PAD_INPUT_VALUE_MAX];
   char name[64];
   uint16_t joykey;
   uint32_t joyaxis;
   unsigned bind;
   if (!rib_pad_input_value(captured->joykey, captured->joyaxis, value, sizeof(value)))
   {
      printf("value none\n");
      return;
   }
   printf("value %s\n", value);
   if (rib_pad_input_parse(value, &joykey, &joyaxis) && rib_pad_input_of(joykey, joyaxis, &bind)
         && rib_pad_input_name(rib_pad_input_captured_pad(), bind, name, sizeof(name)))
      printf("named %s\n", name);
   else
      printf("named none\n");
   if (rib_pad_input_of(captured->joykey, captured->joyaxis, &bind))
      rib_pad_input_value_of(rib_pad_input_id(bind), value, sizeof(value));
   printf("read %s\n", value);
}

static void print_captured(const struct retro_keybind *captured)
{
   const unsigned from = rib_pad_input_captured_pad();
   unsigned bind;
   if (rib_pad_input_on(from, captured->joykey, captured->joyaxis, &bind))
      printf("captured %s", rib_pad_input_id(bind));
   else if (captured->joykey != NO_BTN)
      printf("captured button %u", (unsigned)captured->joykey);
   else if (captured->joyaxis != AXIS_NONE && AXIS_POS_GET(captured->joyaxis) != AXIS_DIR_NONE)
      printf("captured axis +%u", (unsigned)AXIS_POS_GET(captured->joyaxis));
   else if (captured->joyaxis != AXIS_NONE)
      printf("captured axis -%u", (unsigned)AXIS_NEG_GET(captured->joyaxis));
   else
      printf("captured none");
   printf(" on pad %u\n", from + 1);
}

int main(int argc, char **argv)
{
   config_file_t *profile;
   settings_t *settings;
   input_driver_state_t *input;
   struct retro_keybind hotkey;
   struct retro_keybind *captured = &hotkey;
   const char *control = NULL;
   unsigned control_bind = 0;
   unsigned pads = 1;
   int named = 0;
   unsigned port;
   int first = 1;
   int frame;

   while (first + 1 < argc && !strncmp(argv[first], "--", 2))
   {
      if (!strcmp(argv[first], "--named"))
      {
         named = 1;
         first += 1;
         continue;
      }
      if (!strcmp(argv[first], "--controls"))
         control = argv[first + 1];
      else if (!strcmp(argv[first], "--pads"))
         pads = (unsigned)atoi(argv[first + 1]);
      first += 2;
   }
   if (argc < first + 2 || pads < 1 || pads > PADS
         || !(profile = config_file_new_from_path_to_string(argv[first])))
   {
      fprintf(stderr, "usage: pad_capture [--controls POSITION] [--pads N] [--named] PROFILE FRAME...\n");
      return 2;
   }
   retroarch_config_init();
   settings = config_get_ptr();
   settings->uints.input_max_users = pads;
   settings->floats.input_axis_threshold = DEFAULT_AXIS_THRESHOLD;
   input_config_reset();
   input = input_state_get_ptr();
   /* Every pad plays as player 1, as the remap we write on export has it. */
   for (port = 0; port < MAX_USERS; ++port)
      settings->uints.input_remap_port_map[0][port] = port < pads ? port : MAX_USERS;
   for (port = 0; port < pads; ++port)
   {
      settings->uints.input_joypad_index[port] = port;
      input_config_reset_autoconfig_binds(port);
      input_config_set_autoconfig_binds(port, profile);
      input->libretro_input_binds[port] = &input_config_binds[port];
   }
   config_file_free(profile);
   input->primary_joypad = &pad;
   input->current_driver = &keyboard;

   now = 1000000;
   hold(argv[first + 1]);
   if (control)
   {
      if (!rib_pad_input_bind(control, &control_bind))
      {
         fprintf(stderr, "%s is no position of the standard pad\n", control);
         return 2;
      }
      captured = &input_config_binds[0][control_bind];
   }
   if (!(control ? rib_pad_input_bind_start(control_bind, 5) : rib_pad_input_capture_start(&hotkey, 5)))
   {
      fprintf(stderr, "the capture did not start\n");
      return 1;
   }
   for (frame = first + 2; frame < argc; ++frame)
   {
      float remaining;
      now += 16000;
      hold(argv[frame]);
      if (menu_input_rib_bind_poll(now, &remaining, true) != MENU_RIB_BIND_CAPTURED)
         continue;
      print_captured(captured);
      if (named)
         print_named(captured);
      return 0;
   }
   printf("captured nothing\n");
   return 0;
}
