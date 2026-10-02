/* Which pad receives the rumble of a core, through the input layer in the
 * fork: input_set_rumble_state in input_driver.c, which runloop.c passes to
 * a core as its rumble interface, and input_driver_poll, where we read the
 * pads once a frame when a core requests input.
 *
 * Several ports can play as one player (input_remap_port_pN in RetroArch),
 * and in an exported game every pad is player 1. We then send the player's
 * rumble to the pad on which a button was last pressed. We send 0 for both
 * motors to the pad that had it, which would otherwise keep rumbling (SDL for
 * up to five seconds, XInput until it receives a new value), and send the
 * strengths that the core last set to the new pad. Only a new press moves the
 * rumble, not a held button, a stick or the keyboard. With one pad per player,
 * the RetroArch default, the rumble goes to the player's pad as upstream.
 *
 * The joypad driver is a stand-in for three connected pads that records each
 * rumble it receives. The keyboard is a stand-in that reports which key is
 * down. RetroArch does not start. We answer the settings here, and
 * retroarch_unreached.c stands in for the rest of RetroArch. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libretro.h>
#include "configuration.h"
#include "input/input_driver.h"
#include "input/input_remapping.h"

enum { PADS = 3, AXES = 4 };

static settings_t settings;
settings_t *config_get_ptr(void) { return &settings; }

static uint16_t buttons[PADS];
static int16_t axes[PADS][AXES];
static bool key_down;
/* What we sent to the pads since the last check, one line each. */
static char told[1024];

static int32_t pad_button(unsigned pad, uint16_t joykey)
{
   return pad < PADS && joykey < 16 && (buttons[pad] & (1u << joykey));
}

static int16_t pad_axis(unsigned pad, uint32_t joyaxis)
{
   if (pad >= PADS)
      return 0;
   if (AXIS_NEG_GET(joyaxis) < AXES && axes[pad][AXIS_NEG_GET(joyaxis)] < 0)
      return axes[pad][AXIS_NEG_GET(joyaxis)];
   if (AXIS_POS_GET(joyaxis) < AXES && axes[pad][AXIS_POS_GET(joyaxis)] > 0)
      return axes[pad][AXIS_POS_GET(joyaxis)];
   return 0;
}

/* The buttons of the pad as a joypad driver reads them: each of the sixteen
 * on the standard pad through the bind of the player, or else the profile. */
static int16_t pad_state(rarch_joypad_info_t *info, const struct retro_keybind *binds, unsigned port)
{
   int16_t pressed = 0;
   unsigned id;
   for (id = 0; id < RARCH_FIRST_CUSTOM_BIND; ++id)
   {
      const uint16_t joykey = binds[id].joykey != NO_BTN ? binds[id].joykey : info->auto_binds[id].joykey;
      const uint32_t joyaxis = binds[id].joyaxis != AXIS_NONE ? binds[id].joyaxis : info->auto_binds[id].joyaxis;
      if ((joykey != NO_BTN && pad_button(info->joy_idx, joykey))
            || (joyaxis != AXIS_NONE
               && abs(pad_axis(info->joy_idx, joyaxis)) / 32768.0f > info->axis_threshold))
         pressed |= 1 << id;
   }
   return pressed;
}

static bool pad_rumble(unsigned pad, enum retro_rumble_effect effect, uint16_t strength)
{
   size_t used = strlen(told);
   snprintf(told + used, sizeof(told) - used, "pad %u %s %u\n", pad,
         effect == RETRO_RUMBLE_STRONG ? "strong" : "weak", strength);
   return true;
}

static bool pad_query(unsigned pad) { return pad < PADS; }
static void pad_poll(void) { }
static const char *pad_name(unsigned pad) { return "stand-in"; }

static input_device_driver_t pads = { .query_pad = pad_query, .button = pad_button,
   .state = pad_state, .axis = pad_axis, .poll = pad_poll, .set_rumble = pad_rumble,
   .name = pad_name, .ident = "stand-in" };

/* The keyboard: the key bound to player 1's B, held or not. */
static int16_t keyboard_state(void *data, const input_device_driver_t *joypad,
      const input_device_driver_t *sec_joypad, rarch_joypad_info_t *info,
      const retro_keybind_set *binds, bool blocked,
      unsigned port, unsigned device, unsigned idx, unsigned id)
{
   if (device != RETRO_DEVICE_JOYPAD || port != 0 || !key_down)
      return 0;
   return id == RETRO_DEVICE_ID_JOYPAD_MASK ? 1 << RETRO_DEVICE_ID_JOYPAD_B
         : id == RETRO_DEVICE_ID_JOYPAD_B;
}

static void keyboard_poll(void *data) { }
static input_driver_t keyboard = { .poll = keyboard_poll, .input_state = keyboard_state,
   .ident = "stand-in" };

static int failures;

/* What we sent to the pads since the last check, compared with `expected`. */
static void expect(const char *step, const char *expected)
{
   printf("%s\n%s", step, *told ? told : "(nothing)\n");
   if (strcmp(told, expected))
   {
      fprintf(stderr, "FAIL: %s\n  told:\n%s  expected:\n%s", step, *told ? told : "(nothing)\n",
            *expected ? expected : "(nothing)\n");
      ++failures;
   }
   told[0] = '\0';
}

/* A frame: the core requests input, and we read every pad. */
static void frame(void) { input_driver_poll(); }

/* RetroArch with three pads connected and eight players, with empty
 * keyboard and pad binds for each player (the pad profiles put button N at
 * position N of the standard pad and the sticks on axes 0 to 3), and each
 * player played by the ports listed in `played_as`. */
static void start(const unsigned played_as[MAX_USERS], unsigned gain)
{
   input_driver_state_t *input = input_state_get_ptr();
   unsigned port, id;
   memset(input, 0, sizeof(*input));
   memset(&settings, 0, sizeof(settings));
   memset(buttons, 0, sizeof(buttons));
   memset(axes, 0, sizeof(axes));
   key_down = false;
   settings.uints.input_max_users = 8;
   settings.uints.input_rumble_gain = gain;
   settings.bools.input_rumble_enable = true;
   settings.floats.input_axis_threshold = 0.5f;
   settings.ints.input_turbo_bind = -1;
   for (port = 0; port < MAX_USERS; ++port)
   {
      settings.uints.input_joypad_index[port] = port;
      settings.uints.input_remap_ports[port] = played_as[port];
      settings.uints.input_libretro_device[port] = RETRO_DEVICE_JOYPAD;
      input->libretro_input_binds[port] = (const retro_keybind_set *)&input_config_binds[port];
      for (id = 0; id < RARCH_BIND_LIST_END; ++id)
      {
         struct retro_keybind *bind = &input_config_binds[port][id];
         struct retro_keybind *profile = &input_autoconf_binds[port][id];
         bind->key = RETROK_UNKNOWN;
         bind->joykey = profile->joykey = NO_BTN;
         bind->joyaxis = profile->joyaxis = AXIS_NONE;
         bind->mbutton = NO_BTN;
         if (id < RARCH_FIRST_CUSTOM_BIND)
            profile->joykey = id;
         else if (id < RARCH_ANALOG_BIND_LIST_END)
         {
            const unsigned offset = id - RARCH_FIRST_CUSTOM_BIND;
            profile->joyaxis = offset % 2 ? AXIS_NEG(offset / 2) : AXIS_POS(offset / 2);
         }
      }
   }
   input_config_binds[0][RETRO_DEVICE_ID_JOYPAD_B].key = RETROK_z;
   input_remapping_update_port_map();
   input->current_driver = &keyboard;
   input->current_data = &keyboard;
   input->primary_joypad = &pads;
   told[0] = '\0';
}

int main(void)
{
   static const unsigned own_pads[MAX_USERS] = { 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15 };
   /* As written by an export in which every pad is player 1. */
   static const unsigned one_player[MAX_USERS] = { 0, 0, 0, 0, 0, 0, 0, 0, 8, 9, 10, 11, 12, 13, 14, 15 };

   /* One pad per player, the RetroArch default, with the pads of the first
    * two players swapped (input_joypad_indexN). As upstream, the rumble of a
    * player goes to the pad assigned to that player, and a press anywhere
    * changes nothing. */
   start(own_pads, 100);
   settings.uints.input_joypad_index[0] = 1;
   settings.uints.input_joypad_index[1] = 0;
   input_set_rumble_state(0, RETRO_RUMBLE_STRONG, 30000);
   input_set_rumble_state(1, RETRO_RUMBLE_WEAK, 10000);
   expect("one pad per player: each player's rumble on the pad RetroArch gave it",
         "pad 1 strong 30000\npad 0 weak 10000\n");
   buttons[0] = 1 << RETRO_DEVICE_ID_JOYPAD_A;
   frame();
   buttons[0] = 0;
   buttons[1] = 1 << RETRO_DEVICE_ID_JOYPAD_B;
   frame();
   expect("one pad per player: presses on either pad", "");
   input_set_rumble_state(0, RETRO_RUMBLE_STRONG, 0);
   expect("one pad per player: player 1's rumble stopped, on its pad", "pad 1 strong 0\n");

   /* Every pad is player 1. */
   start(one_player, 100);
   input_set_rumble_state(0, RETRO_RUMBLE_STRONG, 30000);
   input_set_rumble_state(0, RETRO_RUMBLE_WEAK, 10000);
   expect("every pad player 1: before any press, the first pad rumbles",
         "pad 0 strong 30000\npad 0 weak 10000\n");
   buttons[1] = 1 << RETRO_DEVICE_ID_JOYPAD_B;
   frame();
   expect("every pad player 1: the second pad presses B",
         "pad 0 strong 0\npad 0 weak 0\npad 1 strong 30000\npad 1 weak 10000\n");
   frame();
   expect("every pad player 1: the second pad holds B", "");
   input_set_rumble_state(0, RETRO_RUMBLE_STRONG, 20000);
   expect("every pad player 1: the core changes the strong motor",
         "pad 1 strong 20000\n");
   axes[0][0] = 0x7fff;
   frame();
   axes[0][0] = 0;
   expect("every pad player 1: the first pad pushes its stick", "");
   key_down = true;
   frame();
   key_down = false;
   expect("every pad player 1: the keyboard presses player 1's B", "");
   buttons[0] = 1 << RETRO_DEVICE_ID_JOYPAD_A;
   frame();
   expect("every pad player 1: the first pad presses A while the second still holds B",
         "pad 1 strong 0\npad 1 weak 0\npad 0 strong 20000\npad 0 weak 10000\n");
   buttons[1] = 0;
   frame();
   buttons[1] = 1 << RETRO_DEVICE_ID_JOYPAD_B;
   frame();
   expect("every pad player 1: the second pad lets B go and presses it again",
         "pad 0 strong 0\npad 0 weak 0\npad 1 strong 20000\npad 1 weak 10000\n");
   buttons[2] = 1 << RETRO_DEVICE_ID_JOYPAD_START;
   frame();
   expect("every pad player 1: the third pad presses Start",
         "pad 1 strong 0\npad 1 weak 0\npad 2 strong 20000\npad 2 weak 10000\n");
   if (input_set_rumble_state(1, RETRO_RUMBLE_STRONG, 30000))
   {
      fprintf(stderr, "FAIL: player 2, whom no pad plays as, was rumbled\n");
      ++failures;
   }
   expect("every pad player 1: the core rumbles player 2, whom no pad plays as", "");

   /* Opening the menu stops the rumble on every pad (CMD_EVENT_RUMBLE_STOP).
    * A button press on a pad afterwards, in the menu or after it, starts
    * nothing, because we stopped the last strengths from the core. */
   start(one_player, 100);
   input_set_rumble_state(0, RETRO_RUMBLE_STRONG, 30000);
   input_set_rumble_state(0, RETRO_RUMBLE_WEAK, 10000);
   input_driver_stop_rumble();
   told[0] = '\0';
   buttons[1] = 1 << RETRO_DEVICE_ID_JOYPAD_B;
   frame();
   expect("every pad player 1, the rumble stopped: the second pad presses B",
         "pad 0 strong 0\npad 0 weak 0\npad 1 strong 0\npad 1 weak 0\n");

   /* We scale the strength from the core by the RetroArch rumble gain once,
    * both when we pass it to the new pad and when it was set. */
   start(one_player, 50);
   input_set_rumble_state(0, RETRO_RUMBLE_STRONG, 30000);
   buttons[1] = 1 << RETRO_DEVICE_ID_JOYPAD_B;
   frame();
   expect("every pad player 1, rumble gain 50%: the second pad presses B",
         "pad 0 strong 15000\npad 0 strong 0\npad 0 weak 0\npad 1 strong 15000\npad 1 weak 0\n");

   if (!failures)
      printf("last pad: a player's rumble follows the pad that last pressed a button, and with one pad per player stays where RetroArch puts it\n");
   return failures ? 1 : 0;
}
