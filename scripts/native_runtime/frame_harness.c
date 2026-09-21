/* Load a libretro core, run a game, press buttons, save frames.
 *
 * We use this program to test automatically whether a game works. We load
 * the exact core file we ship, pass it the content a person would drop in,
 * run frames and write them out, with no window, no audio device and no
 * RetroArch. In a test we can then check that pressing Start changes the
 * menu, and nobody has to watch.
 *
 * This does not replace checking by hand. From a frame we know only that
 * the content loaded and that there are pixels, not that the game feels
 * right.
 *
 *   frame_harness --core CORE --content GAME [options]
 *     --system-dir DIR     the folder with the BIOS files for the core
 *     --save-dir DIR       the folder for the core's saves
 *     --frames N           how many frames to run (default 600)
 *     --shot N:FILE        write frame N as a PPM (repeatable)
 *     --press BUTTON:N:LEN hold BUTTON from frame N for LEN frames (repeatable)
 *     --option KEY=VALUE   set a core option, as we write it into
 *                          core-options.cfg for an exported game (repeatable)
 *     --pad digital|dualshock  the controller in port 1. Some games, for
 *                          example Ape Escape, do not work with a digital
 *                          pad
 *     --spam BUTTON:PERIOD[:UNTIL]  tap BUTTON every PERIOD frames, until frame
 *                          UNTIL if given, so that you can then choose in a
 *                          menu with --press. Tapping gets you past logos and
 *                          menus without knowing in advance where they are.
 *                          BUTTON is start, select, a, b, x, y, l, r, l2, r2,
 *                          l3, r3, up, down, left or right
 *
 * We write frames as binary PPM, so we need no image library here, and we
 * convert them in the calling script. Each failure has its own exit code, so
 * we can tell "the content did not load" from "the core cannot be loaded".
 */

#include <dlfcn.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "libretro.h"

#define MAX_EVENTS 16
#define MAX_SHOTS 16

static const char *system_directory = ".";
static const char *save_directory = ".";
static enum retro_pixel_format pixel_format = RETRO_PIXEL_FORMAT_0RGB1555;
static unsigned current_frame = 0;
/* The controller in port 1. A digital pad is the safe default, but the choice
 * has effects. For example, Ape Escape was made for the DualShock and does not
 * work with any other controller. */
static unsigned pad_device = RETRO_DEVICE_JOYPAD;

/* Core options are the only way to configure a core. When we give no answer
 * to GET_VARIABLE, every option stays at its default with no warning, so
 * without options we could not reproduce an exported game here. */
#define MAX_OPTIONS 16
static struct { const char *key; const char *value; } options[MAX_OPTIONS];
static unsigned option_count = 0;

struct press {
    unsigned id;
    unsigned from;
    unsigned until;
    unsigned period; /* 0 for a single hold */
};
static struct press presses[MAX_EVENTS];
static unsigned press_count = 0;

struct shot {
    unsigned frame;
    const char *path;
};
static struct shot shots[MAX_SHOTS];
static unsigned shot_count = 0;

/* Button names as a player would say them, instead of the API numbers. */
static bool button_id(const char *name, unsigned *out)
{
    static const struct { const char *name; unsigned id; } table[] = {
        { "b", RETRO_DEVICE_ID_JOYPAD_B },         { "y", RETRO_DEVICE_ID_JOYPAD_Y },
        { "select", RETRO_DEVICE_ID_JOYPAD_SELECT },{ "start", RETRO_DEVICE_ID_JOYPAD_START },
        { "up", RETRO_DEVICE_ID_JOYPAD_UP },       { "down", RETRO_DEVICE_ID_JOYPAD_DOWN },
        { "left", RETRO_DEVICE_ID_JOYPAD_LEFT },   { "right", RETRO_DEVICE_ID_JOYPAD_RIGHT },
        { "a", RETRO_DEVICE_ID_JOYPAD_A },         { "x", RETRO_DEVICE_ID_JOYPAD_X },
        { "l", RETRO_DEVICE_ID_JOYPAD_L },         { "r", RETRO_DEVICE_ID_JOYPAD_R },
        { "l2", RETRO_DEVICE_ID_JOYPAD_L2 },       { "r2", RETRO_DEVICE_ID_JOYPAD_R2 },
        { "l3", RETRO_DEVICE_ID_JOYPAD_L3 },       { "r3", RETRO_DEVICE_ID_JOYPAD_R3 },
    };
    for (size_t i = 0; i < sizeof(table) / sizeof(*table); i++)
        if (strcmp(table[i].name, name) == 0) { *out = table[i].id; return true; }
    return false;
}

static void log_printf(enum retro_log_level level, const char *format, ...)
{
    (void)level;
    va_list arguments;
    va_start(arguments, format);
    vfprintf(stderr, format, arguments);
    va_end(arguments);
}

static bool environment(unsigned command, void *data)
{
    switch (command) {
    case RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY:
        *(const char **)data = system_directory;
        return true;
    case RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY:
        *(const char **)data = save_directory;
        return true;
    case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT:
        pixel_format = *(enum retro_pixel_format *)data;
        return true;
    case RETRO_ENVIRONMENT_GET_CAN_DUPE:
        /* Tell the core that we cannot repeat a frame, so that we get new
         * pixels every time. If we accepted repeated frames, we could miss
         * the one frame we have to capture, with no warning. */
        *(bool *)data = false;
        return true;
    case RETRO_ENVIRONMENT_GET_VARIABLE: {
        struct retro_variable *variable = data;
        for (unsigned i = 0; i < option_count; i++) {
            if (!strcmp(options[i].key, variable->key)) {
                variable->value = options[i].value;
                return true;
            }
        }
        return false; /* unset means the core keeps its own default */
    }
    case RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE:
        *(bool *)data = false;
        return true;
    case RETRO_ENVIRONMENT_SET_CONTROLLER_INFO: {
        /* The controllers a core can emulate are those in its own controller
         * table. A device id guessed from the generic libretro constants can
         * be the wrong device, such as "analog" for a PlayStation DualShock. */
        const struct retro_controller_info *ports = data;
        for (unsigned port = 0; ports && ports[port].types; port++) {
            for (unsigned i = 0; i < ports[port].num_types; i++) {
                const struct retro_controller_description *type = &ports[port].types[i];
                if (!type->desc) continue;
                fprintf(stderr, "controller port %u: id=%u  \"%s\"\n",
                        port, type->id, type->desc);
            }
        }
        return true;
    }
    case RETRO_ENVIRONMENT_GET_LOG_INTERFACE:
        ((struct retro_log_callback *)data)->log = log_printf;
        return true;
    /* Refusing an optional request from the core is always safe, and so we
     * provide nothing beyond what this file handles. */
    default:
        return false;
    }
}

/* Write one frame as a binary PPM, converted from the pixel format in
 * use. */
static void write_ppm(const char *path, const void *data, unsigned width,
                      unsigned height, size_t pitch)
{
    FILE *file = fopen(path, "wb");
    if (!file) { fprintf(stderr, "cannot write %s\n", path); return; }
    fprintf(file, "P6\n%u %u\n255\n", width, height);
    for (unsigned y = 0; y < height; y++) {
        const uint8_t *row = (const uint8_t *)data + y * pitch;
        for (unsigned x = 0; x < width; x++) {
            uint8_t rgb[3];
            if (pixel_format == RETRO_PIXEL_FORMAT_XRGB8888) {
                uint32_t pixel = ((const uint32_t *)row)[x];
                rgb[0] = (pixel >> 16) & 0xff;
                rgb[1] = (pixel >> 8) & 0xff;
                rgb[2] = pixel & 0xff;
            } else if (pixel_format == RETRO_PIXEL_FORMAT_RGB565) {
                uint16_t pixel = ((const uint16_t *)row)[x];
                rgb[0] = (uint8_t)(((pixel >> 11) & 0x1f) * 255 / 31);
                rgb[1] = (uint8_t)(((pixel >> 5) & 0x3f) * 255 / 63);
                rgb[2] = (uint8_t)((pixel & 0x1f) * 255 / 31);
            } else { /* 0RGB1555 */
                uint16_t pixel = ((const uint16_t *)row)[x];
                rgb[0] = (uint8_t)(((pixel >> 10) & 0x1f) * 255 / 31);
                rgb[1] = (uint8_t)(((pixel >> 5) & 0x1f) * 255 / 31);
                rgb[2] = (uint8_t)((pixel & 0x1f) * 255 / 31);
            }
            fwrite(rgb, 1, 3, file);
        }
    }
    fclose(file);
}

static unsigned long frames_with_pixels = 0;

static void video_refresh(const void *data, unsigned width, unsigned height, size_t pitch)
{
    if (!data) return; /* a duped frame repeats the previous one */
    frames_with_pixels++;
    for (unsigned i = 0; i < shot_count; i++)
        if (shots[i].frame == current_frame)
            write_ppm(shots[i].path, data, width, height, pitch);
}

static void audio_sample(int16_t left, int16_t right) { (void)left; (void)right; }
static size_t audio_batch(const int16_t *data, size_t frames) { (void)data; return frames; }
static void input_poll(void) {}

static int16_t input_state(unsigned port, unsigned device, unsigned index, unsigned id)
{
    (void)index;
    if (port != 0) return 0;
    /* Answer only button queries. Queries for an analogue pad also cover the
     * stick positions, whose ids overlap the button ids, and an answer from
     * the button table would put a stick at one side on every frame. */
    if (device != RETRO_DEVICE_JOYPAD) return 0;
    for (unsigned i = 0; i < press_count; i++) {
        if (presses[i].id != id) continue;
        if (presses[i].period) {
            if (presses[i].until && current_frame >= presses[i].until) continue;
            /* Release the button between taps. Without a release, the taps
             * make one long press and the repeats have no effect. */
            unsigned phase = (current_frame + presses[i].from) % presses[i].period;
            if (phase < 6) return 1;
        } else if (current_frame >= presses[i].from && current_frame < presses[i].until) {
            return 1;
        }
    }
    return 0;
}

static void *read_file(const char *path, size_t *size)
{
    FILE *file = fopen(path, "rb");
    if (!file) return NULL;
    fseek(file, 0, SEEK_END);
    long length = ftell(file);
    fseek(file, 0, SEEK_SET);
    void *buffer = malloc((size_t)length);
    if (buffer && fread(buffer, 1, (size_t)length, file) != (size_t)length) {
        free(buffer);
        buffer = NULL;
    }
    fclose(file);
    if (buffer) *size = (size_t)length;
    return buffer;
}

int main(int argc, char **argv)
{
    const char *core_path = NULL, *content_path = NULL;
    unsigned frames = 600;

    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--core") && i + 1 < argc) core_path = argv[++i];
        else if (!strcmp(argv[i], "--content") && i + 1 < argc) content_path = argv[++i];
        else if (!strcmp(argv[i], "--system-dir") && i + 1 < argc) system_directory = argv[++i];
        else if (!strcmp(argv[i], "--save-dir") && i + 1 < argc) save_directory = argv[++i];
        else if (!strcmp(argv[i], "--frames") && i + 1 < argc) frames = (unsigned)atoi(argv[++i]);
        else if (!strcmp(argv[i], "--shot") && i + 1 < argc && shot_count < MAX_SHOTS) {
            char *spec = argv[++i], *colon = strchr(spec, ':');
            if (!colon) { fprintf(stderr, "--shot wants FRAME:FILE\n"); return 2; }
            *colon = '\0';
            shots[shot_count].frame = (unsigned)atoi(spec);
            shots[shot_count].path = colon + 1;
            shot_count++;
        } else if (!strcmp(argv[i], "--option") && i + 1 < argc && option_count < MAX_OPTIONS) {
            char *spec = argv[++i];
            char *equals = strchr(spec, '=');
            if (!equals) { fprintf(stderr, "--option wants KEY=VALUE\n"); return 2; }
            *equals = '\0';
            options[option_count].key = spec;
            options[option_count].value = equals + 1;
            option_count++;
        } else if (!strcmp(argv[i], "--pad") && i + 1 < argc) {
            const char *kind = argv[++i];
            /* Plain RETRO_DEVICE_ANALOG is not a PlayStation pad, and with it
             * the PCSX ReARMed log shows "device: none". A DualShock is the
             * first subclass of ANALOG, and that is the id in the core's table. */
            if (!strcmp(kind, "dualshock")) pad_device = RETRO_DEVICE_SUBCLASS(RETRO_DEVICE_ANALOG, 0);
            else if (!strcmp(kind, "digital")) pad_device = RETRO_DEVICE_JOYPAD;
            else if (kind[0] >= '0' && kind[0] <= '9') pad_device = (unsigned)atoi(kind);
            else { fprintf(stderr, "--pad wants digital, dualshock, or a device id\n"); return 2; }
        } else if (!strcmp(argv[i], "--press") && i + 1 < argc && press_count < MAX_EVENTS) {
            char *spec = argv[++i];
            char *first = strchr(spec, ':');
            if (!first) { fprintf(stderr, "--press wants BUTTON:FRAME:LENGTH\n"); return 2; }
            *first = '\0';
            char *second = strchr(first + 1, ':');
            unsigned length = second ? (unsigned)atoi(second + 1) : 10;
            if (second) *second = '\0';
            if (!button_id(spec, &presses[press_count].id)) {
                fprintf(stderr, "unknown button '%s'\n", spec);
                return 2;
            }
            presses[press_count].from = (unsigned)atoi(first + 1);
            presses[press_count].until = presses[press_count].from + length;
            press_count++;
        } else if (!strcmp(argv[i], "--spam") && i + 1 < argc && press_count < MAX_EVENTS) {
            char *spec = argv[++i], *colon = strchr(spec, ':');
            if (!colon) { fprintf(stderr, "--spam wants BUTTON:PERIOD\n"); return 2; }
            *colon = '\0';
            if (!button_id(spec, &presses[press_count].id)) {
                fprintf(stderr, "unknown button '%s'\n", spec);
                return 2;
            }
            char *second = strchr(colon + 1, ':');
            if (second) *second = '\0';
            presses[press_count].period = (unsigned)atoi(colon + 1);
            /* Stagger buttons so they are not all pressed on the same frame. */
            presses[press_count].from = press_count * 7;
            presses[press_count].until = second ? (unsigned)atoi(second + 1) : 0;
            press_count++;
        } else { fprintf(stderr, "unexpected argument %s\n", argv[i]); return 2; }
    }
    if (!core_path || !content_path) {
        fprintf(stderr, "usage: frame_harness --core CORE --content GAME [...]\n");
        return 2;
    }

    void *library = dlopen(core_path, RTLD_NOW | RTLD_LOCAL);
    if (!library) { fprintf(stderr, "%s\n", dlerror()); return 3; }

#define BIND(name) \
    name##_t name##_fn = (name##_t)dlsym(library, #name); \
    if (!name##_fn) { fprintf(stderr, "core has no " #name "\n"); return 4; }
    typedef void (*retro_set_environment_t)(retro_environment_t);
    typedef void (*retro_set_video_refresh_t)(retro_video_refresh_t);
    typedef void (*retro_set_audio_sample_t)(retro_audio_sample_t);
    typedef void (*retro_set_audio_sample_batch_t)(retro_audio_sample_batch_t);
    typedef void (*retro_set_controller_port_device_t)(unsigned, unsigned);
    typedef void (*retro_set_input_poll_t)(retro_input_poll_t);
    typedef void (*retro_set_input_state_t)(retro_input_state_t);
    typedef void (*retro_init_t)(void);
    typedef void (*retro_get_system_info_t)(struct retro_system_info *);
    typedef void (*retro_get_system_av_info_t)(struct retro_system_av_info *);
    typedef bool (*retro_load_game_t)(const struct retro_game_info *);
    typedef void (*retro_run_t)(void);
    typedef void (*retro_unload_game_t)(void);
    typedef void (*retro_deinit_t)(void);
    BIND(retro_set_environment) BIND(retro_set_video_refresh)
    BIND(retro_set_audio_sample) BIND(retro_set_audio_sample_batch)
    BIND(retro_set_input_poll) BIND(retro_set_input_state)
    BIND(retro_set_controller_port_device)
    BIND(retro_init) BIND(retro_get_system_info) BIND(retro_get_system_av_info)
    BIND(retro_load_game) BIND(retro_run) BIND(retro_unload_game) BIND(retro_deinit)
#undef BIND

    retro_set_environment_fn(environment);
    retro_set_video_refresh_fn(video_refresh);
    retro_set_audio_sample_fn(audio_sample);
    retro_set_audio_sample_batch_fn(audio_batch);
    retro_set_input_poll_fn(input_poll);
    retro_set_input_state_fn(input_state);

    struct retro_system_info info = { 0 };
    retro_get_system_info_fn(&info);
    retro_init_fn();

    /* We give a disc core the path to the image and a cartridge core its
     * bytes. We ask the core which one it requires, so one program works for
     * both. */
    struct retro_game_info game = { content_path, NULL, 0, NULL };
    void *content = NULL;
    if (!info.need_fullpath) {
        size_t size = 0;
        content = read_file(content_path, &size);
        if (!content) { fprintf(stderr, "cannot read %s\n", content_path); return 5; }
        game.data = content;
        game.size = size;
    }
    if (!retro_load_game_fn(&game)) {
        fprintf(stderr, "the core refused the content\n");
        return 6;
    }

    /* Set the controller only after the game has loaded, because the pad
     * state in PCSX ReARMed is reset on this call. */
    retro_set_controller_port_device_fn(0, pad_device);

    struct retro_system_av_info av = { 0 };
    retro_get_system_av_info_fn(&av);
    fprintf(stderr, "loaded %s %s: %ux%u\n", info.library_name, info.library_version,
            av.geometry.base_width, av.geometry.base_height);

    for (current_frame = 1; current_frame <= frames; current_frame++)
        retro_run_fn();

    retro_unload_game_fn();
    retro_deinit_fn();
    free(content);
    dlclose(library);
    fprintf(stderr, "ran %u frames, %lu of them produced pixels\n", frames, frames_with_pixels);
    return 0;
}
