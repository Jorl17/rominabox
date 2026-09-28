/* What a core reads when the player presses something in an exported game,
 * with the code in the fork: the input layer (input_driver_poll, where we
 * apply a remap, and input_driver_state_wrapper, which a core calls), the
 * remap loader (input_remapping_load_file in configuration.c) and the bind
 * parsers, given the controls file and the remap that the exporter wrote.
 *
 *   remap_play CONTROLS REMAP PRESS...
 *
 * CONTROLS is an exported controls-defaults.cfg. We bind each position of
 * the pad from its input_player1_<position> lines, as we bind them in the
 * game menu (rib_host_load_bind in host.c). REMAP is the exported .rmp, or
 * "none". Each PRESS is `key:NAME`, a key by its name in the RetroArch
 * config, or `pad:POSITION`, the input of the pad at that position of the
 * standard pad: its button, or for a stick direction its axis moved all the
 * way. The pad is a stand-in whose autoconfig profile puts button N at the
 * position that RetroArch numbers N and the sticks on axes 0 to 3, as in the
 * profile of every physical pad.
 *
 * The output is what the core reads, one line each, by position name: a
 * button read as pressed (`b`), and a stick axis read away from centre, as
 * the direction and how far (`l_x_minus 32767`).
 *
 * RetroArch does not start. The keyboard is a stand-in that reports which
 * keys are down, as every input driver does for RetroArch. The rest of
 * RetroArch is retroarch_unreached.c. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libretro.h>
#include <file/config_file.h>
#include "configuration.h"
#include "runloop.h"
#include "input/input_driver.h"
#include "input/input_keymaps.h"
#include "input/input_remapping.h"

enum { POSITIONS = RARCH_ANALOG_BIND_LIST_END, AXES = 4 };

static bool keys[RETROK_LAST];
static uint32_t buttons;
static int16_t axes[AXES];
static runloop_state_t runloop;

runloop_state_t *runloop_state_get_ptr(void) { return &runloop; }

/* The pad. */
static int32_t pad_button(unsigned port, uint16_t joykey)
{
   return joykey < 32 && (buttons & (1u << joykey));
}

static int16_t pad_axis(unsigned port, uint32_t joyaxis)
{
   if (AXIS_NEG_GET(joyaxis) < AXES && axes[AXIS_NEG_GET(joyaxis)] < 0)
      return axes[AXIS_NEG_GET(joyaxis)];
   if (AXIS_POS_GET(joyaxis) < AXES && axes[AXIS_POS_GET(joyaxis)] > 0)
      return axes[AXIS_POS_GET(joyaxis)];
   return 0;
}

static int16_t pad_state(rarch_joypad_info_t *info, const struct retro_keybind *binds, unsigned port)
{
   int16_t pressed = 0;
   unsigned id;
   for (id = 0; id < RARCH_FIRST_CUSTOM_BIND; ++id)
   {
      const uint16_t joykey = binds[id].joykey != NO_BTN ? binds[id].joykey : info->auto_binds[id].joykey;
      const uint32_t joyaxis = binds[id].joyaxis != AXIS_NONE ? binds[id].joyaxis : info->auto_binds[id].joyaxis;
      if ((joykey != NO_BTN && pad_button(port, joykey))
            || (joyaxis != AXIS_NONE
               && abs(pad_axis(port, joyaxis)) / 32768.0f > info->axis_threshold))
         pressed |= 1 << id;
   }
   return pressed;
}

static bool pad_query(unsigned pad) { return pad == 0; }
static void pad_poll(void) { }
static const char *pad_name(unsigned pad) { return "stand-in"; }

static input_device_driver_t pad = { .query_pad = pad_query, .button = pad_button,
   .state = pad_state, .axis = pad_axis, .poll = pad_poll, .name = pad_name, .ident = "stand-in" };

/* The keyboard, as an input driver reports it: the keys that are down, and
 * the buttons of the pad whose key is down. */
static int16_t keyboard_state(void *data, const input_device_driver_t *joypad,
      const input_device_driver_t *sec_joypad, rarch_joypad_info_t *info,
      const retro_keybind_set *binds, bool blocked,
      unsigned port, unsigned device, unsigned idx, unsigned id)
{
   unsigned bind;
   int16_t pressed = 0;
   if (device == RETRO_DEVICE_KEYBOARD)
      return id < RETROK_LAST && keys[id];
   if (device != RETRO_DEVICE_JOYPAD || blocked)
      return 0;
   for (bind = 0; bind < RARCH_FIRST_CUSTOM_BIND; ++bind)
      if (binds[port][bind].key && binds[port][bind].key < RETROK_LAST && keys[binds[port][bind].key])
         pressed |= 1 << bind;
   return id == RETRO_DEVICE_ID_JOYPAD_MASK ? pressed : (pressed >> id) & 1;
}

static void keyboard_poll(void *data) { }
static input_driver_t keyboard = { .poll = keyboard_poll, .input_state = keyboard_state,
   .ident = "stand-in" };

/* A position by its RetroArch name, as declared in the bind table. */
static int position(const char *name)
{
   unsigned index;
   for (index = 0; index < POSITIONS; ++index)
      if (!strcmp(input_config_bind_map_get_base(index), name))
         return (int)index;
   fprintf(stderr, "no position %s\n", name);
   exit(2);
}

/* The stick axis for a position, and its direction. */
static void stick(int at, unsigned *axis, int *sign)
{
   const unsigned offset = (unsigned)at - RARCH_FIRST_CUSTOM_BIND;
   *axis = offset / 2;
   *sign = offset % 2 ? -1 : 1;
}

/* Bind every position from the controls file as we do in the menu. */
static void bind_positions(config_file_t *config)
{
   unsigned index;
   for (index = 0; index < POSITIONS; ++index)
   {
      char base[64];
      struct retro_keybind *bind = &input_config_binds[0][index];
      const char *name = input_config_bind_map_get_base(index);
      struct config_entry_list *entry;
      bind->key = RETROK_UNKNOWN;
      bind->joykey = NO_BTN;
      bind->joyaxis = AXIS_NONE;
      bind->mbutton = NO_BTN;
      snprintf(base, sizeof(base), "input_player1_%s", name);
      entry = config_get_entry(config, base);
      if (entry && entry->value && *entry->value)
         bind->key = input_config_translate_str_to_rk(entry->value, strlen(entry->value));
      input_config_parse_joy_button(base, config, "input_player1", name, bind);
      input_config_parse_joy_axis(base, config, "input_player1", name, bind);
      input_config_parse_mouse_button(base, config, "input_player1", name, bind);
   }
}

/* The stand-in pad's profile: button N at position N, the sticks on axes. */
static void profile_pad(void)
{
   unsigned index;
   for (index = 0; index < POSITIONS; ++index)
   {
      struct retro_keybind *bind = &input_autoconf_binds[0][index];
      bind->valid = true;
      if (index < RARCH_FIRST_CUSTOM_BIND)
         bind->joykey = index;
      else
      {
         unsigned axis;
         int sign;
         stick((int)index, &axis, &sign);
         bind->joyaxis = sign > 0 ? AXIS_POS(axis) : AXIS_NEG(axis);
      }
   }
}

static void press(const char *what)
{
   if (!strncmp(what, "key:", 4))
   {
      const enum retro_key key = input_config_translate_str_to_rk(what + 4, strlen(what + 4));
      if (key == RETROK_UNKNOWN)
      {
         fprintf(stderr, "no key %s\n", what + 4);
         exit(2);
      }
      keys[key] = true;
   }
   else if (!strncmp(what, "pad:", 4))
   {
      const int at = position(what + 4);
      if (at < RARCH_FIRST_CUSTOM_BIND)
         buttons |= 1u << at;
      else
      {
         unsigned axis;
         int sign;
         stick(at, &axis, &sign);
         axes[axis] = (int16_t)(sign * 0x7fff);
      }
   }
   else
   {
      fprintf(stderr, "press key:NAME or pad:POSITION, not %s\n", what);
      exit(2);
   }
}

int main(int argc, char **argv)
{
   settings_t *settings;
   input_driver_state_t *input = input_state_get_ptr();
   config_file_t *controls;
   unsigned id, stick_index, axis;
   int index;

   if (argc < 3)
   {
      fprintf(stderr, "usage: remap_play CONTROLS REMAP|none PRESS...\n");
      return 2;
   }
   retroarch_config_init();
   settings = config_get_ptr();
   settings->uints.input_max_users = 1;
   settings->bools.input_remap_binds_enable = true;
   settings->floats.input_axis_threshold = 0.5f;
   settings->floats.input_analog_sensitivity = 1.0f;
   settings->ints.input_turbo_bind = -1;
   settings->uints.input_libretro_device[0] = RETRO_DEVICE_JOYPAD;
   input_config_reset();
   profile_pad();
   input->current_driver = &keyboard;
   input->current_data = &keyboard;
   input->primary_joypad = &pad;

   if (!(controls = config_file_new_from_path_to_string(argv[1])))
   {
      fprintf(stderr, "cannot read %s\n", argv[1]);
      return 2;
   }
   bind_positions(controls);
   config_file_free(controls);
   input_remapping_set_defaults(true);
   if (strcmp(argv[2], "none"))
   {
      config_file_t *remap = config_file_new_from_path_to_string(argv[2]);
      if (!remap || !input_remapping_load_file(remap, argv[2]))
      {
         fprintf(stderr, "cannot load the remap %s\n", argv[2]);
         return 2;
      }
      config_file_free(remap);
   }

   for (index = 3; index < argc; ++index)
      press(argv[index]);
   input_driver_poll();

   for (id = 0; id < RARCH_FIRST_CUSTOM_BIND; ++id)
      if (input_driver_state_wrapper(0, RETRO_DEVICE_JOYPAD, 0, id))
         printf("%s\n", input_config_bind_map_get_base(id));
   for (stick_index = 0; stick_index < 2; ++stick_index)
      for (axis = 0; axis < 2; ++axis)
      {
         const int16_t value = input_driver_state_wrapper(0, RETRO_DEVICE_ANALOG, stick_index, axis);
         unsigned minus, plus;
         input_conv_analog_id_to_bind_id(stick_index, axis, minus, plus);
         if (value)
            printf("%s %d\n", input_config_bind_map_get_base(value < 0 ? minus : plus), abs(value));
      }
   return 0;
}
