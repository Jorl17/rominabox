/* What reaches a pad when a core rumbles it, through the input layer in the
 * fork: input_set_rumble_state in input_driver.c, which is the rumble
 * interface of a core (set up in runloop.c) and is also called for cheats.
 * The joypad drivers are three stand-ins that record what they receive: a
 * primary, the secondary paired with it in a build with MFi (the player has
 * none), and a primary that scales strength itself, as with set_rumble_gain.
 * The player's drivers (HID on the Mac, XInput and DirectInput on Windows)
 * scale nothing themselves, and in a sandboxed game DirectInput reaches the
 * launcher's relay only through the driver.
 *
 *   rumble_gate on|off
 *
 * sets input_rumble_enable in RetroArch to that value, requests a strong
 * rumble of 30000 on the first pad through each pairing as a core does, and
 * prints what each driver received, one line each:
 *
 *   <driver> <strength>
 *
 * RetroArch does not start. We answer the settings here, and
 * retroarch_unreached.c stands in for the rest of RetroArch. */
#include <stdio.h>
#include <string.h>
#include <libretro.h>
#include "configuration.h"
#include "input/input_driver.h"

#define ASKED 30000

static settings_t settings;
static int told[3];

settings_t *config_get_ptr(void) { return &settings; }

static bool primary_rumble(unsigned pad, enum retro_rumble_effect effect, uint16_t strength)
{
   told[0] = strength;
   return true;
}

static bool secondary_rumble(unsigned pad, enum retro_rumble_effect effect, uint16_t strength)
{
   told[1] = strength;
   return true;
}

static bool scaling_rumble(unsigned pad, enum retro_rumble_effect effect, uint16_t strength)
{
   told[2] = strength;
   return true;
}

static bool scaling_gain(unsigned pad, unsigned gain) { return true; }

static input_device_driver_t primary = { .set_rumble = primary_rumble, .ident = "primary" };
static input_device_driver_t secondary = { .set_rumble = secondary_rumble, .ident = "secondary" };
static input_device_driver_t scaling = { .set_rumble = scaling_rumble,
   .set_rumble_gain = scaling_gain, .ident = "scaling" };

/* A core's strong rumble on the first pad, with these drivers attached,
 * written after the frame in which the core set it, as in runloop.c. */
static void rumble_with(const input_device_driver_t *first, const input_device_driver_t *second)
{
   input_driver_state_t *input = input_state_get_ptr();
   input->primary_joypad = first;
   input->secondary_joypad = second;
   input_set_rumble_state(0, RETRO_RUMBLE_STRONG, ASKED);
   input_driver_flush_rumble();
}

int main(int argc, char **argv)
{
   if (argc != 2 || (strcmp(argv[1], "on") && strcmp(argv[1], "off")))
   {
      fprintf(stderr, "usage: rumble_gate on|off\n");
      return 2;
   }
   settings.bools.input_rumble_enable = !strcmp(argv[1], "on");
   /* The RetroArch default: the full strength that the core requests. */
   settings.uints.input_rumble_gain = 100;
   told[0] = told[1] = told[2] = -1;
   rumble_with(&primary, &secondary);
   rumble_with(&scaling, NULL);
   printf("primary %d\nsecondary %d\nscaling %d\n", told[0], told[1], told[2]);
   return 0;
}
