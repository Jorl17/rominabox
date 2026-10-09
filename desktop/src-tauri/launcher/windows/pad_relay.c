/* The launcher's side of a sandboxed game's controllers (pad_relay.h). On one
 * thread we use DirectInput and a window of our own, and find the controllers
 * as in RetroArch's joypad driver each time the game builds its list. We wait
 * on that thread until a request arrives from the game, then perform it on
 * the controllers and reply. Once a frame, the request is to read them all.
 *
 * DirectInput cannot rumble a DualSense or an Xbox pad, so on the thread we
 * also use SDL's joysticks. For a controller that SDL can rumble, the game's
 * rumble effects drive SDL's motors instead of DirectInput's. */
#define WIN32_LEAN_AND_MEAN
#define DIRECTINPUT_VERSION 0x0800
#include <windows.h>
#include <dinput.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define SDL_MAIN_HANDLED
#include <SDL.h>

#include <objbase.h>

#include "game_data_requests.h"
#include "pad_relay.h"
#include "../launch.h"
#include "../../../../vendor/retroarch/rominabox_launch.h"
#include "../../../../vendor/retroarch/rominabox_pad_relay.h"

/* The block is writable from inside the sandbox, where the game runs, so we
 * trust nothing in it. In the launcher we keep our own count of the
 * controllers, copy each request once, and check that copy before we act on
 * it. */
struct PadRelay {
    /* The game's data folder, for its export and import. */
    char data_dir[LAUNCH_PATH_CAP];
    HANDLE block;
    rib_pad_relay *view;
    HANDLE request;
    HANDLE reply;
    HANDLE stop;
    HANDLE ready;
    HANDLE thread;
    /* The thread's own. */
    HWND window;
    IDirectInput8A *input;
    DWORD pad_count;
    IDirectInputDevice8A *devices[RIB_PAD_RELAY_PADS];
    IDirectInputEffect *effects[RIB_PAD_RELAY_PADS][RIB_PAD_RELAY_EFFECTS];
    /* Each controller's model, DirectInput's MAKELONG(vendor, product). */
    DWORD models[RIB_PAD_RELAY_PADS];
    /* Whether SDL's joysticks started, each controller in SDL when SDL can
     * rumble it, the motor for each of its effects, and the levels of the
     * motors, low frequency then high. */
    int sdl;
    SDL_Joystick *rumblers[RIB_PAD_RELAY_PADS];
    int motors[RIB_PAD_RELAY_PADS][RIB_PAD_RELAY_EFFECTS];
    Uint16 levels[RIB_PAD_RELAY_PADS][2];
};

/* The motor for an effect: no SDL motor (we make the effect in DirectInput),
 * or one of SDL's two, which match the effects along X (strong rumble) and
 * along Y (weak rumble) in RetroArch's joypad driver. */
enum { NO_MOTOR, LOW_FREQUENCY_MOTOR, HIGH_FREQUENCY_MOTOR };

/* SDL's joysticks, on this thread, for rumble. SDL's DirectInput backend
 * requires a window from SDL's video, and we build this SDL without video.
 * Also, we read DirectInput's controllers on this thread for the game. With
 * raw input, an Xbox pad rumbles through SDL only after a press, and we read
 * no presses with this SDL, while with XInput the rumble works at once. The
 * window of this thread is never in front. */
static int start_sdl(void) {
    SDL_SetHint(SDL_HINT_DIRECTINPUT_ENABLED, "0");
    SDL_SetHint(SDL_HINT_JOYSTICK_RAWINPUT, "0");
    SDL_SetHint(SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, "1");
    return SDL_Init(SDL_INIT_JOYSTICK) == 0;
}

/* The SDL controller for the `index`th DirectInput controller, when rumble
 * works through SDL. Among the controllers of that model in SDL's list, it is
 * the one with as many before it as in DirectInput's list. */
static SDL_Joystick *rumbler_for(PadRelay *relay, DWORD index) {
    DWORD model = relay->models[index];
    DWORD before = 0;
    DWORD seen = 0;
    DWORD earlier;
    int listed;
    if (!relay->sdl)
        return NULL;
    for (earlier = 0; earlier < index; earlier++)
        before += relay->models[earlier] == model;
    SDL_JoystickUpdate();
    for (listed = 0; listed < SDL_NumJoysticks(); listed++) {
        SDL_Joystick *found;
        if (SDL_JoystickGetDeviceVendor(listed) != LOWORD(model)
            || SDL_JoystickGetDeviceProduct(listed) != HIWORD(model) || seen++ != before)
            continue;
        found = SDL_JoystickOpen(listed);
        if (found && SDL_JoystickHasRumble(found))
            return found;
        if (found)
            SDL_JoystickClose(found);
        return NULL;
    }
    return NULL;
}

static void rumble(PadRelay *relay, DWORD pad) {
    SDL_JoystickRumble(relay->rumblers[pad], relay->levels[pad][0], relay->levels[pad][1], 0);
}

typedef struct {
    rib_pad_relay_pad *pad;
    DWORD count;
} FoundAxes;

static BOOL CALLBACK found_axis(const DIDEVICEOBJECTINSTANCEA *axis, void *context) {
    FoundAxes *found = context;
    if (found->count == RIB_PAD_RELAY_AXES)
        return DIENUM_STOP;
    found->pad->axes[found->count++] = *axis;
    return DIENUM_CONTINUE;
}

/* Set up a controller as in RetroArch's joypad driver, with the joystick data
 * format and background access. */
static BOOL CALLBACK found_pad(const DIDEVICEINSTANCEA *device, void *context) {
    PadRelay *relay = context;
    DWORD index = relay->pad_count;
    FoundAxes axes = {0};
    IDirectInputDevice8A *opened = NULL;
    if (index == RIB_PAD_RELAY_PADS)
        return DIENUM_STOP;
    if (FAILED(IDirectInput8_CreateDevice(relay->input, &device->guidInstance, &opened, NULL)))
        return DIENUM_CONTINUE;
    IDirectInputDevice8_SetDataFormat(opened, &c_dfDIJoystick2);
    IDirectInputDevice8_SetCooperativeLevel(opened, relay->window, DISCL_EXCLUSIVE | DISCL_BACKGROUND);
    axes.pad = &relay->view->pads[index];
    axes.pad->device = *device;
    IDirectInputDevice8_EnumObjects(opened, found_axis, &axes, DIDFT_ABSAXIS);
    axes.pad->axis_count = axes.count;
    IDirectInputDevice8_Acquire(opened);
    relay->devices[index] = opened;
    relay->models[index] = device->guidProduct.Data1;
    relay->rumblers[index] = rumbler_for(relay, index);
    relay->view->pad_count = ++relay->pad_count;
    return DIENUM_CONTINUE;
}

/* Read one controller, as in RetroArch's joypad driver. When we cannot poll
 * the controller, even after we acquire it again, its state stays empty. */
static void read_pad(PadRelay *relay, DWORD index) {
    rib_pad_relay_pad *pad = &relay->view->pads[index];
    IDirectInputDevice8A *device = relay->devices[index];
    memset(&pad->state, 0, sizeof pad->state);
    pad->result = DI_OK;
    if (FAILED(IDirectInputDevice8_Poll(device))
        && (FAILED(IDirectInputDevice8_Acquire(device)) || FAILED(IDirectInputDevice8_Poll(device))))
        return;
    pad->result = IDirectInputDevice8_GetDeviceState(device, sizeof pad->state, &pad->state);
}

/* Read every controller once. */
static void read_pads(PadRelay *relay) {
    DWORD index;
    for (index = 0; index < relay->pad_count; index++)
        read_pad(relay, index);
}

/* The effect described in `carried`, in DirectInput's form. Its pointers
 * point into the four places given. We check its axis count in the caller. */
static void uncarry_effect(const rib_pad_relay_effect *carried, DIEFFECT *effect, DWORD *axes, LONG *directions,
                           DIENVELOPE *envelope, DICONSTANTFORCE *force) {
    memset(effect, 0, sizeof *effect);
    memcpy(axes, carried->axes, sizeof carried->axes);
    memcpy(directions, carried->directions, sizeof carried->directions);
    *envelope = carried->envelope;
    *force = carried->force;
    effect->dwSize = sizeof *effect;
    effect->dwFlags = carried->flags;
    effect->dwDuration = carried->duration;
    effect->dwSamplePeriod = carried->sample_period;
    effect->dwGain = carried->gain;
    effect->dwTriggerButton = carried->trigger_button;
    effect->dwTriggerRepeatInterval = carried->trigger_repeat_interval;
    effect->dwStartDelay = carried->start_delay;
    effect->cAxes = carried->axis_count;
    effect->rgdwAxes = axes;
    effect->rglDirection = directions;
    effect->lpEnvelope = carried->has_envelope ? envelope : NULL;
    effect->cbTypeSpecificParams = sizeof *force;
    effect->lpvTypeSpecificParams = force;
}

static void drop_effect(PadRelay *relay, DWORD pad, DWORD slot) {
    IDirectInputEffect *effect = relay->effects[pad][slot];
    int motor = relay->motors[pad][slot];
    if (motor != NO_MOTOR) {
        relay->motors[pad][slot] = NO_MOTOR;
        relay->levels[pad][motor - LOW_FREQUENCY_MOTOR] = 0;
        rumble(relay, pad);
    }
    if (!effect)
        return;
    IDirectInputEffect_Release(effect);
    relay->effects[pad][slot] = NULL;
}

/* Replace the last list, and every effect made on it, with the controllers
 * attached now, each set up again. */
static void list_pads(PadRelay *relay) {
    DWORD index;
    DWORD slot;
    for (index = 0; index < relay->pad_count; index++) {
        for (slot = 0; slot < RIB_PAD_RELAY_EFFECTS; slot++)
            drop_effect(relay, index, slot);
        IDirectInputDevice8_Unacquire(relay->devices[index]);
        IDirectInputDevice8_Release(relay->devices[index]);
        relay->devices[index] = NULL;
        if (relay->rumblers[index])
            SDL_JoystickClose(relay->rumblers[index]);
        relay->rumblers[index] = NULL;
    }
    relay->pad_count = 0;
    relay->view->pad_count = 0;
    IDirectInput8_EnumDevices(relay->input, DI8DEVCLASS_GAMECTRL, found_pad, relay, DIEDFL_ATTACHEDONLY);
}

static HRESULT set_range(IDirectInputDevice8A *device, const rib_pad_relay_ask *ask) {
    DIPROPRANGE range;
    range.diph.dwSize = sizeof range;
    range.diph.dwHeaderSize = sizeof range.diph;
    range.diph.dwHow = DIPH_BYID;
    range.diph.dwObj = ask->item;
    range.lMin = ask->range_min;
    range.lMax = ask->range_max;
    return IDirectInputDevice8_SetProperty(device, DIPROP_RANGE, &range.diph);
}

/* Set an axis's range and read the controller again in it, because the game
 * may still have a state from the last read, in the old range, and use it
 * before its next request. */
static HRESULT set_range_and_read(PadRelay *relay, const rib_pad_relay_ask *ask) {
    HRESULT answered = set_range(relay->devices[ask->pad], ask);
    if (SUCCEEDED(answered))
        read_pad(relay, ask->pad);
    return answered;
}

/* Make a constant force in the controller's first free slot, and return that
 * slot in `slot`. */
static HRESULT make_effect(PadRelay *relay, const rib_pad_relay_ask *ask, DWORD *slot) {
    DIEFFECT effect;
    DWORD axes[RIB_PAD_RELAY_EFFECT_AXES];
    LONG directions[RIB_PAD_RELAY_EFFECT_AXES];
    DIENVELOPE envelope;
    DICONSTANTFORCE force;
    for (*slot = 0; *slot < RIB_PAD_RELAY_EFFECTS; (*slot)++)
        if (!relay->effects[ask->pad][*slot] && relay->motors[ask->pad][*slot] == NO_MOTOR)
            break;
    if (*slot == RIB_PAD_RELAY_EFFECTS)
        return DIERR_DEVICEFULL;
    if (relay->rumblers[ask->pad]) {
        relay->motors[ask->pad][*slot] = ask->effect.axes[0] == DIJOFS_X ? LOW_FREQUENCY_MOTOR : HIGH_FREQUENCY_MOTOR;
        return DI_OK;
    }
    uncarry_effect(&ask->effect, &effect, axes, directions, &envelope, &force);
    return IDirectInputDevice8_CreateEffect(relay->devices[ask->pad], &GUID_ConstantForce, &effect,
                                            &relay->effects[ask->pad][*slot], NULL);
}

static HRESULT set_effect(IDirectInputEffect *changed, const rib_pad_relay_ask *ask) {
    DIEFFECT effect;
    DWORD axes[RIB_PAD_RELAY_EFFECT_AXES];
    LONG directions[RIB_PAD_RELAY_EFFECT_AXES];
    DIENVELOPE envelope;
    DICONSTANTFORCE force;
    uncarry_effect(&ask->effect, &effect, axes, directions, &envelope, &force);
    return IDirectInputEffect_SetParameters(changed, &effect, ask->flags);
}

/* Perform `ask`, the launcher's copy of the request, and reply with
 * DirectInput's result. `slot` is the place of a new effect. */
static HRESULT do_ask(PadRelay *relay, const rib_pad_relay_ask *ask, DWORD *slot) {
    int pad = ask->pad < relay->pad_count;
    IDirectInputEffect *effect = pad && ask->item < RIB_PAD_RELAY_EFFECTS ? relay->effects[ask->pad][ask->item] : NULL;
    int carried = ask->effect.axis_count <= RIB_PAD_RELAY_EFFECT_AXES;
    int motor = pad && ask->item < RIB_PAD_RELAY_EFFECTS ? relay->motors[ask->pad][ask->item] : NO_MOTOR;
    if (!relay->input)
        return DIERR_NOTINITIALIZED;
    /* For an effect on an SDL motor, we set the motor to the effect's gain
     * when it starts, and to zero when it stops or is released. */
    if (motor != NO_MOTOR) {
        switch (ask->what) {
        case RIB_PAD_RELAY_SET_EFFECT:
            if (!(ask->flags & DIEP_START))
                return DI_OK;
            relay->levels[ask->pad][motor - LOW_FREQUENCY_MOTOR] =
                (Uint16)((ask->effect.gain > DI_FFNOMINALMAX ? DI_FFNOMINALMAX : ask->effect.gain) * 65535u
                         / DI_FFNOMINALMAX);
            rumble(relay, ask->pad);
            return DI_OK;
        case RIB_PAD_RELAY_STOP_EFFECT:
            relay->levels[ask->pad][motor - LOW_FREQUENCY_MOTOR] = 0;
            rumble(relay, ask->pad);
            return DI_OK;
        case RIB_PAD_RELAY_DROP_EFFECT:
            drop_effect(relay, ask->pad, ask->item);
            return DI_OK;
        default:
            break;
        }
    }
    switch (ask->what) {
    case RIB_PAD_RELAY_LIST:
        list_pads(relay);
        return DI_OK;
    case RIB_PAD_RELAY_READ:
        read_pads(relay);
        return DI_OK;
    case RIB_PAD_RELAY_SET_RANGE:
        return pad ? set_range_and_read(relay, ask) : DIERR_INVALIDPARAM;
    case RIB_PAD_RELAY_MAKE_EFFECT:
        return pad && carried ? make_effect(relay, ask, slot) : DIERR_INVALIDPARAM;
    case RIB_PAD_RELAY_SET_EFFECT:
        return effect && carried ? set_effect(effect, ask) : DIERR_INVALIDPARAM;
    case RIB_PAD_RELAY_STOP_EFFECT:
        return effect ? IDirectInputEffect_Stop(effect) : DIERR_INVALIDPARAM;
    case RIB_PAD_RELAY_DROP_EFFECT:
        if (!effect)
            return DIERR_INVALIDPARAM;
        drop_effect(relay, ask->pad, ask->item);
        return DI_OK;
    case RIB_PAD_RELAY_EXPORT_DATA:
    case RIB_PAD_RELAY_CHOOSE_IMPORT:
    case RIB_PAD_RELAY_CONFIRM_IMPORT:
        /* The game's data, which answer_ask handles before this. */
        break;
    }
    return DIERR_UNSUPPORTED;
}

/* Copy the game's request once, perform it, and reply in the block. */
static void answer_ask(PadRelay *relay) {
    rib_pad_relay_ask ask;
    DWORD slot = 0;
    HRESULT answered;
    memcpy(&ask, &relay->view->ask, sizeof ask);
    if (ask.what == RIB_PAD_RELAY_EXPORT_DATA || ask.what == RIB_PAD_RELAY_CHOOSE_IMPORT
        || ask.what == RIB_PAD_RELAY_CONFIRM_IMPORT) {
        game_data_request(ask.what, relay->data_dir, relay->window, (HWND)(uintptr_t)ask.window,
                          &relay->view->data);
        relay->view->ask.answer = DI_OK;
        return;
    }
    answered = do_ask(relay, &ask, &slot);
    if (ask.what == RIB_PAD_RELAY_MAKE_EFFECT)
        relay->view->ask.item = slot;
    relay->view->ask.answer = answered;
}

static DWORD WINAPI answer(void *context) {
    PadRelay *relay = context;
    HINSTANCE instance = GetModuleHandleW(NULL);
    WNDCLASSW kind = {0};
    HANDLE waits[2];
    DWORD index;
    DWORD slot;
    /* The file dialogs of the game's data need COM on this thread. */
    CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
    kind.lpfnWndProc = DefWindowProcW;
    kind.hInstance = instance;
    kind.lpszClassName = L"ROM-in-a-Box controllers";
    RegisterClassW(&kind);
    relay->window = CreateWindowExW(0, kind.lpszClassName, L"", WS_POPUP, 0, 0, 0, 0, NULL, NULL, instance, NULL);
    if (relay->window)
        DirectInput8Create(instance, DIRECTINPUT_VERSION, &IID_IDirectInput8A, (void **)&relay->input, NULL);
    SetEvent(relay->ready);
    /* After we let the game start, because this would otherwise delay it. We
     * answer the game's first request only after this. */
    relay->sdl = start_sdl();

    waits[0] = relay->request;
    waits[1] = relay->stop;
    for (;;) {
        DWORD woke = MsgWaitForMultipleObjects(2, waits, FALSE, INFINITE, QS_ALLINPUT);
        if (woke == WAIT_OBJECT_0) {
            answer_ask(relay);
            SetEvent(relay->reply);
        } else if (woke == WAIT_OBJECT_0 + 2) {
            MSG message;
            while (PeekMessageW(&message, NULL, 0, 0, PM_REMOVE)) {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        } else {
            break;
        }
    }

    for (index = 0; index < relay->pad_count; index++) {
        for (slot = 0; slot < RIB_PAD_RELAY_EFFECTS; slot++)
            drop_effect(relay, index, slot);
        IDirectInputDevice8_Unacquire(relay->devices[index]);
        IDirectInputDevice8_Release(relay->devices[index]);
        if (relay->rumblers[index])
            SDL_JoystickClose(relay->rumblers[index]);
    }
    if (relay->sdl)
        SDL_Quit();
    if (relay->input)
        IDirectInput8_Release(relay->input);
    if (relay->window)
        DestroyWindow(relay->window);
    CoUninitialize();
    return 0;
}

static void close_relay(PadRelay *relay) {
    if (relay->view)
        UnmapViewOfFile(relay->view);
    if (relay->block)
        CloseHandle(relay->block);
    if (relay->request)
        CloseHandle(relay->request);
    if (relay->reply)
        CloseHandle(relay->reply);
    if (relay->stop)
        CloseHandle(relay->stop);
    if (relay->ready)
        CloseHandle(relay->ready);
    if (relay->thread)
        CloseHandle(relay->thread);
    free(relay);
}

PadRelay *pad_relay_start(const char *data_dir) {
    /* Inheritable, so they reach the game through the launcher inside the
     * sandbox, where the named objects on this side cannot be opened. */
    SECURITY_ATTRIBUTES inherit = {sizeof inherit, NULL, TRUE};
    PadRelay *relay = calloc(1, sizeof *relay);
    char handles[96];
    HANDLE started[2];
    if (!relay)
        return NULL;
    snprintf(relay->data_dir, sizeof relay->data_dir, "%s", data_dir);
    relay->block = CreateFileMappingW(INVALID_HANDLE_VALUE, &inherit, PAGE_READWRITE, 0, sizeof(rib_pad_relay), NULL);
    relay->view = relay->block
        ? MapViewOfFile(relay->block, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(rib_pad_relay)) : NULL;
    relay->request = CreateEventW(&inherit, FALSE, FALSE, NULL);
    relay->reply = CreateEventW(&inherit, FALSE, FALSE, NULL);
    relay->stop = CreateEventW(NULL, TRUE, FALSE, NULL);
    relay->ready = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!relay->view || !relay->request || !relay->reply || !relay->stop || !relay->ready
        || !(relay->thread = CreateThread(NULL, 0, answer, relay, 0, NULL))) {
        close_relay(relay);
        return NULL;
    }
    /* We set up DirectInput before the first request from the game. */
    started[0] = relay->ready;
    started[1] = relay->thread;
    WaitForMultipleObjects(2, started, FALSE, INFINITE);
    snprintf(handles, sizeof handles, "%llu,%llu,%llu", (unsigned long long)(uintptr_t)relay->block,
             (unsigned long long)(uintptr_t)relay->request, (unsigned long long)(uintptr_t)relay->reply);
    SetEnvironmentVariableA(RIB_ENV_PAD_RELAY, handles);
    return relay;
}

void pad_relay_stop(PadRelay *relay) {
    if (!relay)
        return;
    SetEvent(relay->stop);
    WaitForSingleObject(relay->thread, INFINITE);
    close_relay(relay);
}
