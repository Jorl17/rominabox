/* The controller relay between a sandboxed Windows game and its launcher, on
 * the local DirectInput: the launcher side
 * (desktop/src-tauri/launcher/windows/pad_relay.c) and the DirectInput
 * stand-in of the player (vendor/retroarch/input/drivers/rominabox_dinput.c)
 * in one process, communicating through the block and the events as a game
 * and its launcher do. Without a controller, we skip and report the checks
 * that require one. We build and run this from scripts/test_pad_relay.py. */
#define WIN32_LEAN_AND_MEAN
#define DIRECTINPUT_VERSION 0x0800
#include <windows.h>
#include <dinput.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

#include "pad_relay.h"
#include "rominabox_pad_relay.h"
#include "input/drivers/rominabox_dinput.h"
#include "rominabox_launch.h"

static int failures;
static char logged[65536];

#define CHECK(condition, ...) \
   do { \
      if (!(condition)) { \
         failures++; \
         printf("FAIL %s:%d: ", __FILE__, __LINE__); \
         printf(__VA_ARGS__); \
         printf("\n"); \
      } \
   } while (0)

/* The RetroArch log, where the stand-in writes, kept for the checks. */
static void keep(const char *fmt, va_list ap) {
   size_t used = strlen(logged);
   vsnprintf(logged + used, sizeof logged - used, fmt, ap);
}

void RARCH_LOG(const char *fmt, ...) {
   va_list ap;
   va_start(ap, fmt);
   keep(fmt, ap);
   va_end(ap);
}

void RARCH_ERR(const char *fmt, ...) {
   va_list ap;
   va_start(ap, fmt);
   keep(fmt, ap);
   va_end(ap);
}

typedef struct {
   GUID found[32];
   int count;
} Listed;

static BOOL CALLBACK list_one(const DIDEVICEINSTANCEA *device, void *context) {
   Listed *listed = context;
   if (listed->count < 32)
      listed->found[listed->count++] = device->guidInstance;
   return DIENUM_CONTINUE;
}

static Listed list(IDirectInput8A *input, DWORD kind) {
   Listed listed = {0};
   IDirectInput8_EnumDevices(input, kind, list_one, &listed, DIEDFL_ATTACHEDONLY);
   return listed;
}

static int same(const Listed *a, const Listed *b) {
   int i, j;
   if (a->count != b->count)
      return 0;
   for (i = 0; i < a->count; i++) {
      int found = 0;
      for (j = 0; j < b->count; j++)
         found |= IsEqualGUID(&a->found[i], &b->found[j]);
      if (!found)
         return 0;
   }
   return 1;
}

typedef struct {
   DWORD ids[16];
   int count;
} Axes;

static BOOL CALLBACK axis_one(const DIDEVICEOBJECTINSTANCEA *axis, void *context) {
   Axes *axes = context;
   if (axes->count < 16)
      axes->ids[axes->count++] = axis->dwType;
   return DIENUM_CONTINUE;
}

static IDirectInput8A *direct_input(void) {
   IDirectInput8A *input = NULL;
   HRESULT made = DirectInput8Create(GetModuleHandleA(NULL), DIRECTINPUT_VERSION, &IID_IDirectInput8A,
                                     (void **)&input, NULL);
   CHECK(SUCCEEDED(made), "DirectInput8Create answered 0x%08lx", (unsigned long)made);
   return input;
}

/* A controller through the stand-in: we set it up, read it and request rumble
 * from it, as in the RetroArch joypad driver. */
static void check_pad(IDirectInput8A *through, const GUID *guid) {
   IDirectInputDevice8A *pad = NULL;
   DIDEVICEINSTANCEA info;
   Axes axes = {0};
   DIJOYSTATE2 state;
   DIPROPRANGE range;
   IDirectInputEffect *effect = NULL;
   DIEFFECT parameters;
   DICONSTANTFORCE force = {0};
   DIENVELOPE envelope = {sizeof envelope, 5000, 250000, 0, 250000};
   DWORD axis = DIJOFS_X;
   LONG direction = 0;
   HRESULT answered;
   LONG *values;
   int i;

   CHECK(SUCCEEDED(IDirectInput8_CreateDevice(through, guid, &pad, NULL)), "a relayed controller was not made");
   if (!pad)
      return;
   info.dwSize = sizeof info;
   CHECK(IDirectInputDevice8_GetDeviceInfo(pad, &info) == DI_OK && IsEqualGUID(&info.guidInstance, guid),
         "the controller does not describe itself");
   printf("pad relay: %s\n", info.tszProductName);
   CHECK(strstr(logged, info.tszProductName) != NULL, "opening \"%s\" was not logged", info.tszProductName);
   CHECK(IDirectInputDevice8_SetDataFormat(pad, &c_dfDIJoystick2) == DI_OK, "the joystick format was refused");
   CHECK(IDirectInputDevice8_SetCooperativeLevel(pad, NULL, DISCL_EXCLUSIVE | DISCL_BACKGROUND) == DI_OK,
         "the cooperative level was refused");
   IDirectInputDevice8_EnumObjects(pad, axis_one, &axes, DIDFT_ABSAXIS);
   CHECK(axes.count > 0, "the controller has no axes");

   /* A range that the launcher sets on the controller: every axis then reads
    * within it. The DirectInput default is 0 to 65535. */
   range.diph.dwSize = sizeof range;
   range.diph.dwHeaderSize = sizeof range.diph;
   range.diph.dwHow = DIPH_BYID;
   range.lMin = -1000;
   range.lMax = 1000;
   for (i = 0; i < axes.count; i++) {
      range.diph.dwObj = axes.ids[i];
      answered = IDirectInputDevice8_SetProperty(pad, DIPROP_RANGE, &range.diph);
      CHECK(answered == DI_OK, "setting axis %d's range answered 0x%08lx", i, (unsigned long)answered);
   }
   range.diph.dwObj = 0xffffff;
   answered = IDirectInputDevice8_SetProperty(pad, DIPROP_RANGE, &range.diph);
   CHECK(FAILED(answered), "a range on an axis the controller does not have was accepted");

   CHECK(IDirectInputDevice8_Poll(pad) == DI_OK, "polling failed");
   answered = IDirectInputDevice8_GetDeviceState(pad, sizeof state, &state);
   CHECK(answered == DI_OK, "reading answered 0x%08lx", (unsigned long)answered);
   values = &state.lX;
   for (i = 0; i < 6; i++)
      CHECK(values[i] >= -1000 && values[i] <= 1000, "axis value %ld is outside the range set", values[i]);
   CHECK(IDirectInputDevice8_GetDeviceState(pad, sizeof state - 1, &state) == DIERR_INVALIDPARAM,
         "a state of the wrong size was read");

   /* Rumble as in the RetroArch joypad driver: the game gets the same answer
    * from DirectInput as the launcher. */
   memset(&parameters, 0, sizeof parameters);
   parameters.dwSize = sizeof parameters;
   parameters.dwFlags = DIEFF_CARTESIAN | DIEFF_OBJECTOFFSETS;
   parameters.dwDuration = INFINITE;
   parameters.dwTriggerButton = DIEB_NOTRIGGER;
   parameters.cAxes = 1;
   parameters.rgdwAxes = &axis;
   parameters.rglDirection = &direction;
   parameters.lpEnvelope = &envelope;
   parameters.cbTypeSpecificParams = sizeof force;
   parameters.lpvTypeSpecificParams = &force;
   answered = IDirectInputDevice8_CreateEffect(pad, &GUID_ConstantForce, &parameters, &effect, NULL);
   printf("pad relay: rumble effect answered 0x%08lx\n", (unsigned long)answered);
   CHECK(SUCCEEDED(answered) == (effect != NULL), "an effect's answer and the effect disagree");
   if (effect) {
      /* The weakest rumble possible, so a connected controller that we rumble
       * through the launcher does not shake during a test. */
      parameters.dwGain = 1;
      CHECK(IDirectInputEffect_SetParameters(effect, &parameters, DIEP_GAIN | DIEP_START) == DI_OK,
            "starting the rumble failed");
      CHECK(IDirectInputEffect_Stop(effect) == DI_OK, "stopping the rumble failed");
      IDirectInputEffect_Release(effect);
   }
   effect = NULL;
   CHECK(IDirectInputDevice8_CreateEffect(pad, &GUID_Sine, &parameters, &effect, NULL) == DIERR_UNSUPPORTED
            && !effect,
         "an effect the relay does not carry was made");
   IDirectInputDevice8_Release(pad);
}

/* One request as from a game, directly through the block and the events. */
static HRESULT ask_directly(rib_pad_relay *view, HANDLE request, HANDLE reply) {
   SetEvent(request);
   if (WaitForSingleObject(reply, 5000) != WAIT_OBJECT_0)
      return E_FAIL;
   return view->ask.answer;
}

/* The game writes the block, and we use the sandbox because we do not trust
 * the game. Whatever the game writes, in the launcher we reject any request
 * that is not about its controllers and effects, and keep answering. */
static void check_hostile_game(void) {
   char named[96];
   unsigned long long block = 0, request = 0, reply = 0;
   rib_pad_relay *view;
   DWORD pads;
   HANDLE asks, answers;
   HRESULT answered;
   CHECK(GetEnvironmentVariableA(RIB_ENV_PAD_RELAY, named, sizeof named) > 0
            && sscanf(named, "%llu,%llu,%llu", &block, &request, &reply) == 3,
         "the relay was not named");
   view = MapViewOfFile((HANDLE)(uintptr_t)block, FILE_MAP_ALL_ACCESS, 0, 0, sizeof *view);
   CHECK(view != NULL, "the block could not be mapped");
   if (!view)
      return;
   asks = (HANDLE)(uintptr_t)request;
   answers = (HANDLE)(uintptr_t)reply;
   pads = view->pad_count;

   view->pad_count = 1000;
   memset(&view->ask, 0, sizeof view->ask);
   view->ask.what = RIB_PAD_RELAY_SET_RANGE;
   view->ask.pad = 999;
   answered = ask_directly(view, asks, answers);
   CHECK(answered == DIERR_INVALIDPARAM, "a range on controller 999 answered 0x%08lx", (unsigned long)answered);

   view->ask.what = RIB_PAD_RELAY_STOP_EFFECT;
   view->ask.pad = 0;
   view->ask.item = 0xffffffff;
   answered = ask_directly(view, asks, answers);
   CHECK(answered == DIERR_INVALIDPARAM, "stopping effect 0xffffffff answered 0x%08lx", (unsigned long)answered);

   memset(&view->ask, 0, sizeof view->ask);
   view->ask.what = RIB_PAD_RELAY_MAKE_EFFECT;
   view->ask.effect.axis_count = 1000;
   answered = ask_directly(view, asks, answers);
   CHECK(answered == DIERR_INVALIDPARAM, "an effect along 1000 axes answered 0x%08lx", (unsigned long)answered);

   view->ask.what = (rib_pad_relay_asking)77;
   answered = ask_directly(view, asks, answers);
   CHECK(answered == DIERR_UNSUPPORTED, "ask 77 answered 0x%08lx", (unsigned long)answered);

   view->ask.what = RIB_PAD_RELAY_READ;
   answered = ask_directly(view, asks, answers);
   CHECK(answered == DI_OK, "a read after all that answered 0x%08lx", (unsigned long)answered);
   view->pad_count = pads;
   UnmapViewOfFile(view);
}

int main(int argc, char **argv) {
   IDirectInput8A *alone = direct_input();
   IDirectInput8A *real = direct_input();
   IDirectInput8A *through;
   PadRelay *relay;
   Listed pads, relayed, keyboards;
   int i;
   if (!alone || !real)
      return 1;

   /* A game started outside a sandbox uses DirectInput directly. */
   CHECK(!GetEnvironmentVariableA(RIB_ENV_PAD_RELAY, NULL, 0), "%s is set before the test names it",
         RIB_ENV_PAD_RELAY);
   CHECK(rib_dinput_for_game(alone) == alone, "DirectInput was wrapped with no relay named");

   /* The game's data folder, for the requests about its data. */
   relay = pad_relay_start(argc > 1 ? argv[1] : ".");
   CHECK(relay != NULL, "the launcher's relay did not start");
   if (!relay)
      return 1;
   /* A game inherits the name of the relay when it starts. In one process the
    * C runtime has already copied the environment that the player reads, so
    * we pass the name in the same way as for a started game. */
   {
      char named[96];
      CHECK(GetEnvironmentVariableA(RIB_ENV_PAD_RELAY, named, sizeof named) > 0, "the relay was not named");
      _putenv_s(RIB_ENV_PAD_RELAY, named);
   }
   through = rib_dinput_for_game(real);
   CHECK(through != real, "DirectInput was not wrapped with a relay named");

   /* In the game we list the controllers that DirectInput lists outside the
    * sandbox, and every other kind of device as DirectInput lists it. */
   pads = list(alone, DI8DEVCLASS_GAMECTRL);
   relayed = list(through, DI8DEVCLASS_GAMECTRL);
   CHECK(same(&pads, &relayed), "the game lists %d controller(s), DirectInput %d", relayed.count, pads.count);
   keyboards = list(alone, DI8DEVCLASS_KEYBOARD);
   {
      Listed forwarded = list(through, DI8DEVCLASS_KEYBOARD);
      IDirectInputDevice8A *keyboard = NULL;
      CHECK(same(&keyboards, &forwarded), "the game lists %d keyboard(s), DirectInput %d", forwarded.count,
            keyboards.count);
      CHECK(SUCCEEDED(IDirectInput8_CreateDevice(through, &GUID_SysKeyboard, &keyboard, NULL)) && keyboard,
            "the keyboard did not come from DirectInput");
      if (keyboard)
         IDirectInputDevice8_Release(keyboard);
   }

   if (relayed.count == 0)
      printf("pad relay: no controller is connected: setting one up, reading it and its rumble were not checked\n");
   for (i = 0; i < relayed.count; i++)
      check_pad(through, &relayed.found[i]);

   /* Whenever Windows reports that a controller was added or removed, the
    * joypad driver releases every controller and lists them again. The
    * launcher lists them again, and we set up and read each as the first time. */
   {
      LARGE_INTEGER frequency, start, end;
      Listed again;
      QueryPerformanceFrequency(&frequency);
      QueryPerformanceCounter(&start);
      again = list(through, DI8DEVCLASS_GAMECTRL);
      QueryPerformanceCounter(&end);
      printf("pad relay: listing again took %.1f ms\n",
             (double)(end.QuadPart - start.QuadPart) * 1000.0 / (double)frequency.QuadPart);
      CHECK(same(&relayed, &again), "listed again, the game has %d controller(s), before %d", again.count,
            relayed.count);
      for (i = 0; i < again.count; i++)
         check_pad(through, &again.found[i]);
   }
   check_hostile_game();

   /* With the launcher gone, the game loses its controllers and reports it,
    * instead of waiting on every read. */
   pad_relay_stop(relay);
   if (relayed.count) {
      IDirectInputDevice8A *pad = NULL;
      DIJOYSTATE2 state;
      IDirectInput8_CreateDevice(through, &relayed.found[0], &pad, NULL);
      CHECK(pad && IDirectInputDevice8_GetDeviceState(pad, sizeof state, &state) == DIERR_INPUTLOST,
            "a read with the launcher gone was not lost");
      CHECK(pad && IDirectInputDevice8_Poll(pad) == DIERR_INPUTLOST, "polling with the launcher gone succeeded");
      if (pad)
         IDirectInputDevice8_Release(pad);
      CHECK(strstr(logged, "stopped answering") != NULL, "the launcher going was not logged");
   }
   IDirectInput8_Release(through);
   IDirectInput8_Release(alone);
   if (failures)
      return 1;
   printf("pad relay: ok\n");
   return 0;
}
