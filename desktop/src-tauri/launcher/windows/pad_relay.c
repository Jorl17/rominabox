/* The launcher's side of a sandboxed game's controllers (pad_relay.h). On one
 * thread we use DirectInput and a window of our own, and find the controllers
 * as in RetroArch's joypad driver. We wait on that thread until a request
 * arrives from the game, then perform it on the controllers and reply. Once a
 * frame, the request is to read every controller. */
#define WIN32_LEAN_AND_MEAN
#define DIRECTINPUT_VERSION 0x0800
#include <windows.h>
#include <dinput.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "pad_relay.h"
#include "../../../../vendor/retroarch/rominabox_launch.h"
#include "../../../../vendor/retroarch/rominabox_pad_relay.h"

/* The block is writable from inside the sandbox, where the game runs, so we
 * trust nothing in it. In the launcher we keep our own count of the
 * controllers, copy each request once, and check that copy before we act on
 * it. */
struct PadRelay {
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
};

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
    relay->view->pad_count = ++relay->pad_count;
    return DIENUM_CONTINUE;
}

/* Read every controller once, as in RetroArch's joypad driver. When we cannot
 * poll a controller, even after we acquire it again, its state stays empty. */
static void read_pads(PadRelay *relay) {
    DWORD index;
    for (index = 0; index < relay->pad_count; index++) {
        rib_pad_relay_pad *pad = &relay->view->pads[index];
        IDirectInputDevice8A *device = relay->devices[index];
        memset(&pad->state, 0, sizeof pad->state);
        pad->result = DI_OK;
        if (FAILED(IDirectInputDevice8_Poll(device))
            && (FAILED(IDirectInputDevice8_Acquire(device)) || FAILED(IDirectInputDevice8_Poll(device))))
            continue;
        pad->result = IDirectInputDevice8_GetDeviceState(device, sizeof pad->state, &pad->state);
    }
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
    if (!effect)
        return;
    IDirectInputEffect_Release(effect);
    relay->effects[pad][slot] = NULL;
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

/* Make a constant force in the controller's first free slot, and return that
 * slot in `slot`. */
static HRESULT make_effect(PadRelay *relay, const rib_pad_relay_ask *ask, DWORD *slot) {
    DIEFFECT effect;
    DWORD axes[RIB_PAD_RELAY_EFFECT_AXES];
    LONG directions[RIB_PAD_RELAY_EFFECT_AXES];
    DIENVELOPE envelope;
    DICONSTANTFORCE force;
    for (*slot = 0; *slot < RIB_PAD_RELAY_EFFECTS; (*slot)++)
        if (!relay->effects[ask->pad][*slot])
            break;
    if (*slot == RIB_PAD_RELAY_EFFECTS)
        return DIERR_DEVICEFULL;
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
    if (!relay->input)
        return DIERR_NOTINITIALIZED;
    switch (ask->what) {
    case RIB_PAD_RELAY_READ:
        read_pads(relay);
        return DI_OK;
    case RIB_PAD_RELAY_SET_RANGE:
        return pad ? set_range(relay->devices[ask->pad], ask) : DIERR_INVALIDPARAM;
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
    }
    return DIERR_UNSUPPORTED;
}

/* Copy the game's request once, perform it, and reply in the block. */
static void answer_ask(PadRelay *relay) {
    rib_pad_relay_ask ask;
    DWORD slot = 0;
    HRESULT answered;
    memcpy(&ask, &relay->view->ask, sizeof ask);
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
    kind.lpfnWndProc = DefWindowProcW;
    kind.hInstance = instance;
    kind.lpszClassName = L"ROM-in-a-Box controllers";
    RegisterClassW(&kind);
    relay->window = CreateWindowExW(0, kind.lpszClassName, L"", WS_POPUP, 0, 0, 0, 0, NULL, NULL, instance, NULL);
    if (relay->window
        && SUCCEEDED(DirectInput8Create(instance, DIRECTINPUT_VERSION, &IID_IDirectInput8A, (void **)&relay->input, NULL)))
        IDirectInput8_EnumDevices(relay->input, DI8DEVCLASS_GAMECTRL, found_pad, relay, DIEDFL_ATTACHEDONLY);
    SetEvent(relay->ready);

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

    for (index = 0; index < RIB_PAD_RELAY_PADS; index++)
        for (slot = 0; slot < RIB_PAD_RELAY_EFFECTS; slot++)
            drop_effect(relay, index, slot);
    for (index = 0; index < RIB_PAD_RELAY_PADS; index++)
        if (relay->devices[index]) {
            IDirectInputDevice8_Unacquire(relay->devices[index]);
            IDirectInputDevice8_Release(relay->devices[index]);
        }
    if (relay->input)
        IDirectInput8_Release(relay->input);
    if (relay->window)
        DestroyWindow(relay->window);
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

PadRelay *pad_relay_start(void) {
    /* Inheritable, so they reach the game through the launcher inside the
     * sandbox, where the named objects on this side cannot be opened. */
    SECURITY_ATTRIBUTES inherit = {sizeof inherit, NULL, TRUE};
    PadRelay *relay = calloc(1, sizeof *relay);
    char handles[96];
    HANDLE started[2];
    if (!relay)
        return NULL;
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
    /* We find the controllers before the first request from the game. */
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
