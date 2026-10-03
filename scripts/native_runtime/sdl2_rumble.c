/* What we pass to SDL in the SDL2 joypad driver of RetroArch
 * (input/drivers_joypad/sdl2_joypad.c, used by the Mac player) when a core
 * sets the two motors of a pad. The driver comes from the fork. Here we stand
 * in for SDL, with one controller that rumbles through SDL_JoystickRumble and
 * has no haptic device, like a DualSense. A core sets each motor with a
 * separate call, and SDL_JoystickRumble sets both at once, so each call must
 * also pass the strength of the other motor. Otherwise a game that sets both
 * motors gets only the last one (libretro/RetroArch#17570).
 *
 * RetroArch does not start. retroarch_unreached.c stands in for the rest of
 * it. */
#define SDL_MAIN_HANDLED
#include <stdarg.h>
#include <stdio.h>
#include <SDL.h>
#include <libretro.h>
#include "input/input_driver.h"
#include "tasks/tasks_internal.h"
#include "verbosity.h"

static char joystick;
static char controller;
static unsigned rumbles;
static Uint16 told_low;
static Uint16 told_high;

/* ---- SDL, with one controller ------------------------------------------- */

Uint32 SDLCALL SDL_WasInit(Uint32 flags) { (void)flags; return 0; }
int SDLCALL SDL_Init(Uint32 flags) { (void)flags; return 0; }
/* No haptic subsystem, as on a Mac the DualSense has no haptic device. */
int SDLCALL SDL_InitSubSystem(Uint32 flags) { return (flags & SDL_INIT_HAPTIC) ? -1 : 0; }
SDL_bool SDLCALL SDL_SetHint(const char *name, const char *value) { (void)name; (void)value; return SDL_TRUE; }
const char *SDLCALL SDL_GetError(void) { return "stand-in"; }
int SDLCALL SDL_NumJoysticks(void) { return 1; }
SDL_bool SDLCALL SDL_IsGameController(int index) { (void)index; return SDL_TRUE; }
SDL_GameController *SDLCALL SDL_GameControllerOpen(int index) { (void)index; return (SDL_GameController*)&controller; }
SDL_Joystick *SDLCALL SDL_GameControllerGetJoystick(SDL_GameController *pad) { (void)pad; return (SDL_Joystick*)&joystick; }
const char *SDLCALL SDL_GameControllerNameForIndex(int index) { (void)index; return "DualSense Wireless Controller"; }
const char *SDLCALL SDL_JoystickNameForIndex(int index) { (void)index; return "DualSense Wireless Controller"; }
SDL_JoystickGUID SDLCALL SDL_JoystickGetGUID(SDL_Joystick *pad) { SDL_JoystickGUID guid = {{0}}; (void)pad; return guid; }
SDL_Haptic *SDLCALL SDL_HapticOpenFromJoystick(SDL_Joystick *pad) { (void)pad; return NULL; }
SDL_bool SDLCALL SDL_GameControllerHasSensor(SDL_GameController *pad, SDL_SensorType type) { (void)pad; (void)type; return SDL_FALSE; }

int SDLCALL SDL_JoystickRumble(SDL_Joystick *pad, Uint16 low, Uint16 high, Uint32 duration)
{
   (void)pad;
   (void)duration;
   rumbles++;
   told_low  = low;
   told_high = high;
   return 0;
}

/* Names that the driver uses and that this program never reaches. */
void SDLCALL SDL_FlushEvents(Uint32 min, Uint32 max) { (void)min; (void)max; }
void SDLCALL SDL_PumpEvents(void) { }
int SDLCALL SDL_PeepEvents(SDL_Event *events, int count, SDL_eventaction action, Uint32 min, Uint32 max) { (void)events; (void)count; (void)action; (void)min; (void)max; return 0; }
void SDLCALL SDL_GameControllerClose(SDL_GameController *pad) { (void)pad; }
void SDLCALL SDL_JoystickClose(SDL_Joystick *pad) { (void)pad; }
SDL_Joystick *SDLCALL SDL_JoystickOpen(int index) { (void)index; return (SDL_Joystick*)&joystick; }
Sint16 SDLCALL SDL_GameControllerGetAxis(SDL_GameController *pad, SDL_GameControllerAxis axis) { (void)pad; (void)axis; return 0; }
Uint8 SDLCALL SDL_GameControllerGetButton(SDL_GameController *pad, SDL_GameControllerButton button) { (void)pad; (void)button; return 0; }
int SDLCALL SDL_GameControllerGetSensorData(SDL_GameController *pad, SDL_SensorType type, float *data, int count) { (void)pad; (void)type; (void)data; (void)count; return -1; }
int SDLCALL SDL_GameControllerSetSensorEnabled(SDL_GameController *pad, SDL_SensorType type, SDL_bool on) { (void)pad; (void)type; (void)on; return -1; }
Sint16 SDLCALL SDL_JoystickGetAxis(SDL_Joystick *pad, int axis) { (void)pad; (void)axis; return 0; }
Uint8 SDLCALL SDL_JoystickGetButton(SDL_Joystick *pad, int button) { (void)pad; (void)button; return 0; }
Uint8 SDLCALL SDL_JoystickGetHat(SDL_Joystick *pad, int hat) { (void)pad; (void)hat; return 0; }
int SDLCALL SDL_JoystickNumAxes(SDL_Joystick *pad) { (void)pad; return 0; }
int SDLCALL SDL_JoystickNumBalls(SDL_Joystick *pad) { (void)pad; return 0; }
int SDLCALL SDL_JoystickNumButtons(SDL_Joystick *pad) { (void)pad; return 0; }
int SDLCALL SDL_JoystickNumHats(SDL_Joystick *pad) { (void)pad; return 0; }
void SDLCALL SDL_HapticClose(SDL_Haptic *haptic) { (void)haptic; }
int SDLCALL SDL_HapticEffectSupported(SDL_Haptic *haptic, SDL_HapticEffect *effect) { (void)haptic; (void)effect; return SDL_FALSE; }
int SDLCALL SDL_HapticNewEffect(SDL_Haptic *haptic, SDL_HapticEffect *effect) { (void)haptic; (void)effect; return -1; }
int SDLCALL SDL_HapticRunEffect(SDL_Haptic *haptic, int effect, Uint32 iterations) { (void)haptic; (void)effect; (void)iterations; return -1; }
int SDLCALL SDL_HapticUpdateEffect(SDL_Haptic *haptic, int effect, SDL_HapticEffect *data) { (void)haptic; (void)effect; (void)data; return -1; }

/* ---- RetroArch, as far as connecting a pad requires ---------------------- */

bool input_autoconfigure_connect(const char *name, const char *display_name, const char *phys,
      const char *driver, unsigned port, unsigned vid, unsigned pid)
{
   (void)name; (void)display_name; (void)phys; (void)driver; (void)port; (void)vid; (void)pid;
   return true;
}

bool input_autoconfigure_disconnect(unsigned port, const char *name) { (void)port; (void)name; return true; }

static void say(const char *fmt, va_list args) { vfprintf(stderr, fmt, args); }
void RARCH_LOG(const char *fmt, ...) { va_list args; va_start(args, fmt); say(fmt, args); va_end(args); }
void RARCH_WARN(const char *fmt, ...) { va_list args; va_start(args, fmt); say(fmt, args); va_end(args); }
void RARCH_ERR(const char *fmt, ...) { va_list args; va_start(args, fmt); say(fmt, args); va_end(args); }

/* ---- the motors ---------------------------------------------------------- */

static int failures;

/* The core sets one motor. We must then pass both current strengths to SDL. */
static void set(enum retro_rumble_effect motor, uint16_t strength, Uint16 low, Uint16 high, const char *what)
{
   unsigned before = rumbles;
   bool answered   = sdl2_joypad.set_rumble(0, motor, strength);
   if (rumbles != before + 1 || told_low != low || told_high != high)
   {
      fprintf(stderr, "FAIL %s: SDL was told low %u, high %u, not low %u, high %u\n",
            what, told_low, told_high, low, high);
      failures++;
   }
   else if (!answered)
   {
      fprintf(stderr, "FAIL %s: the pad rumbled, and the driver answered that it did not\n", what);
      failures++;
   }
   else
      printf("ok   %s: SDL told low %u, high %u\n", what, low, high);
}

int main(void)
{
   if (!sdl2_joypad.init(NULL))
   {
      fprintf(stderr, "FAIL the driver did not start\n");
      return 1;
   }
   set(RETRO_RUMBLE_STRONG, 40000, 40000, 0, "the strong motor set");
   set(RETRO_RUMBLE_WEAK, 20000, 40000, 20000, "the weak motor set beside it");
   set(RETRO_RUMBLE_STRONG, 0, 0, 20000, "the strong motor stopped, the weak one kept");
   set(RETRO_RUMBLE_WEAK, 0, 0, 0, "both stopped");
   return failures ? 1 : 0;
}
