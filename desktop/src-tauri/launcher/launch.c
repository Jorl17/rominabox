#include "launch.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "accounts_folder.h"
#include "player_settings.h"
#include "portable_fs.h"
#include "shipped_files.h"
#include "shipped_settings.h"
#include "../../../vendor/retroarch/rominabox_launch.h"

/* The fields of the plan, the app's files and the places a config line can
 * refer to, as declared in launch_contract.inc, and the menu's files, as
 * declared in the player's declarations.inc. */
#define RIB_APP_FILE(name, path) static const char app_##name[] = path;
#define RIB_CORE_FILE(platform, file) static const char core_##platform[] = file;
#define RIB_PLAN_FIELD(name, field) static const char plan_##name[] = field;
#define RIB_PLAN_MARK(name, line) static const char plan_mark_##name[] = line;
#define RIB_TOKEN(name, token) static const char token_##name[] = token;
#include "launch_contract.inc"
#define RIB_FILE(name, file) static const char menu_##name[] = file;
#define RIB_DATA_FILE(name, file) static const char menu_data_##name[] = file;
#include "../../../vendor/retroarch/menu/drivers/rmlui/declarations.inc"

/* A folder that we ship in the app, and its counterpart in the game's data. */
typedef struct {
    const char *app;
    const char *data;
} Shipped;

#define PATH_CAP LAUNCH_PATH_CAP
#define LINE_CAP 8192
#define MANAGED_CAP 64

/* RIB_ENV_QUIET is one switch for an automated run, and a person who opens
 * the game does not set it. Without it, a screenshot run would open an output
 * device and play sound. ROMINABOX_SOUND turns sound on in any case, and we
 * read it only in the launcher. */
#define ROMINABOX_SOUND_ENV "ROMINABOX_SOUND"

/* The separators in a path: `/` everywhere, and `\` on Windows too. */
static int is_separator(char c) {
#if defined(_WIN32)
    return c == '/' || c == '\\';
#elif defined(__APPLE__) || defined(__unix__)
    return c == '/';
#else
#error "the launcher has no path separators declared for this platform"
#endif
}

/* The core's file name in the app, with this platform's library naming. */
static const char *core_file(void) {
#if defined(_WIN32)
    return core_Windows;
#elif defined(__APPLE__)
    return core_Macos;
#else
#error "the launcher has no core file declared for this platform"
#endif
}

void rominabox_launch_die(const char *message) {
    fprintf(stderr, "ROM-in-a-Box: %s\n", message);
    exit(1);
}

static void die_errno(const char *message) {
    fprintf(stderr, "ROM-in-a-Box: %s: %s\n", message, strerror(errno));
    exit(1);
}

#define die rominabox_launch_die

static int starts_with(const char *value, const char *prefix) {
    return strncmp(value, prefix, strlen(prefix)) == 0;
}

static void join_path(char *out, size_t out_cap, const char *left, const char *right) {
    if (fs_join(out, out_cap, left, right) != 0)
        die("a path does not fit");
}

/* We accept a path that exists, whatever it is: a folder, or a link to one,
 * like /var on macOS. */
static void mkdir_one(const char *path) {
    if (fs_make_directory(path) == 0 || errno == EEXIST)
        return;
    die_errno(path);
}

/* The length of the root at the start of `path`, which we never create:
 * `/`, or on Windows a drive (C:\) or a share (\\server\share\). */
static size_t root_length(const char *path) {
#if defined(_WIN32)
    if (((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z'))
        && path[1] == ':' && is_separator(path[2]))
        return 3;
    if (is_separator(path[0]) && is_separator(path[1])) {
        size_t index = 2;
        int separators = 0;
        while (path[index] && separators < 2) {
            if (is_separator(path[index]))
                separators++;
            index++;
        }
        return index;
    }
    return 0;
#elif defined(__APPLE__) || defined(__unix__)
    return path[0] == '/' ? 1 : 0;
#else
#error "the launcher has no path roots declared for this platform"
#endif
}

static void mkdir_p(const char *path) {
    char buffer[PATH_CAP];
    size_t length = strlen(path);
    size_t index;
    size_t start = root_length(path);
    if (length == 0 || length >= sizeof buffer)
        die("a directory path does not fit");
    memcpy(buffer, path, length + 1);
    for (index = start > 1 ? start : 1; index < length; index++) {
        char separator = buffer[index];
        if (!is_separator(separator))
            continue;
        buffer[index] = '\0';
        mkdir_one(buffer);
        buffer[index] = separator;
    }
    mkdir_one(buffer);
}

void rominabox_launch_join(char *out, size_t out_cap, const char *left, const char *right) {
    join_path(out, out_cap, left, right);
}

void rominabox_launch_make_directories(const char *path) {
    mkdir_p(path);
}

static char *read_file(const char *path, size_t *length_out) {
    FILE *file = fs_open(path, "rb");
    long length = 0;
    char *body;
    if (!file)
        return NULL;
    if (fseek(file, 0, SEEK_END) != 0 || (length = ftell(file)) < 0 || length > 1024 * 1024) {
        fclose(file);
        die("a launch file is unreadable or too large");
    }
    rewind(file);
    body = malloc((size_t)length + 1);
    if (!body)
        die("out of memory");
    if (fread(body, 1, (size_t)length, file) != (size_t)length) {
        fclose(file);
        die("a launch file changed while it was read");
    }
    fclose(file);
    body[length] = '\0';
    if (length_out)
        *length_out = (size_t)length;
    return body;
}

typedef struct {
    char *key;
    char *line;
    int frozen;
} ConfigLine;

/* The lines allowed in a controls file: RetroArch's own bindings and the keys
 * for controls files in the player's declarations.inc. We read a player
 * setting from its own file listed in the plan, never from a controls file. */
static int allowed_player_key(const char *key) {
    if (starts_with(key, "input_player"))
        return 1;
#define RIB_CONTROLS_KEY(name, controls_key) \
    if (strcmp(key, controls_key) == 0)      \
        return 1;
#define RIB_CONTROLS_KEYS(name, prefix) \
    if (starts_with(key, prefix))       \
        return 1;
#include "../../../vendor/retroarch/menu/drivers/rmlui/declarations.inc"
    return 0;
}

static char *config_key(const char *line) {
    const char *cursor = line;
    const char *end;
    char *key;
    while (*cursor == ' ' || *cursor == '\t')
        cursor++;
    if (*cursor == '\0' || *cursor == '#')
        return NULL;
    end = cursor;
    while (*end && *end != ' ' && *end != '\t' && *end != '=')
        end++;
    if (end == cursor)
        return NULL;
    key = malloc((size_t)(end - cursor) + 1);
    if (!key)
        die("out of memory");
    memcpy(key, cursor, (size_t)(end - cursor));
    key[end - cursor] = '\0';
    return key;
}

static void add_line(ConfigLine **lines, size_t *count, size_t *capacity, char *key, char *line, int frozen) {
    if (*count == *capacity) {
        *capacity = *capacity ? *capacity * 2 : 64;
        *lines = realloc(*lines, *capacity * sizeof **lines);
        if (!*lines)
            die("out of memory");
    }
    (*lines)[*count].key = key;
    (*lines)[*count].line = line;
    (*lines)[*count].frozen = frozen;
    (*count)++;
}

static void replace_token(char *line, const char *token, const char *value) {
    char buffer[LINE_CAP];
    char *found;
    size_t token_len = strlen(token);
    if (strlen(line) >= sizeof buffer)
        die("a config line does not fit");
    memcpy(buffer, line, strlen(line) + 1);
    while ((found = strstr(line, token))) {
        size_t head = (size_t)(found - line);
        int wrote = snprintf(
            buffer,
            sizeof buffer,
            "%.*s%s%s",
            (int)head,
            line,
            value,
            found + token_len
        );
        if (wrote < 0 || (size_t)wrote >= sizeof buffer)
            die("a config line does not fit");
        memcpy(line, buffer, (size_t)wrote + 1);
    }
}

/* In a config line, `$resources_dir` stands for the game's own files in the
 * app's Contents/Resources, and `$data_dir` for its data. */
static void load_base(ConfigLine **lines, size_t *count, size_t *capacity, const char *text, const char *data_dir, const char *resources_dir) {
    const char *cursor = text;
    while (*cursor) {
        const char *end = strchr(cursor, '\n');
        size_t length = end ? (size_t)(end - cursor) : strlen(cursor);
        char *line = malloc(LINE_CAP);
        char *key;
        if (!line)
            die("out of memory");
        if (length >= LINE_CAP)
            die("a config line does not fit");
        memcpy(line, cursor, length);
        line[length] = '\0';
        replace_token(line, token_ResourcesDir, resources_dir);
        replace_token(line, token_DataDir, data_dir);
        key = config_key(line);
        add_line(lines, count, capacity, key, line, 1);
        if (!end)
            break;
        cursor = end + 1;
    }
}

static int find_key(ConfigLine *lines, size_t count, const char *key, int frozen_only) {
    size_t index;
    for (index = 0; index < count; index++) {
        if (!lines[index].key || strcmp(lines[index].key, key) != 0)
            continue;
        if (frozen_only && !lines[index].frozen)
            continue;
        if (!frozen_only && lines[index].frozen)
            continue;
        return (int)index;
    }
    return -1;
}

static void apply_player_file(ConfigLine **lines, size_t *count, size_t *capacity, const char *path) {
    FILE *file = fs_open(path, "r");
    char raw[LINE_CAP];
    if (!file)
        return;
    while (fgets(raw, sizeof raw, file)) {
        char *key;
        char *line;
        size_t length = strlen(raw);
        int existing;
        while (length > 0 && (raw[length - 1] == '\n' || raw[length - 1] == '\r'))
            raw[--length] = '\0';
        key = config_key(raw);
        if (!key)
            continue;
        if (!allowed_player_key(key)) {
            free(key);
            continue;
        }
        if (find_key(*lines, *count, key, 1) >= 0) {
            free(key);
            continue;
        }
        line = malloc(length + 1);
        if (!line)
            die("out of memory");
        memcpy(line, raw, length + 1);
        existing = find_key(*lines, *count, key, 0);
        if (existing >= 0) {
            free((*lines)[existing].key);
            free((*lines)[existing].line);
            (*lines)[existing].key = key;
            (*lines)[existing].line = line;
            continue;
        }
        add_line(lines, count, capacity, key, line, 0);
    }
    fclose(file);
}

/* We write in binary mode on every platform, so LF stays LF on Windows. */
static void write_config(const char *path, ConfigLine *lines, size_t count) {
    FILE *file = fs_open(path, "wb");
    size_t index;
    if (!file)
        die_errno(path);
    for (index = 0; index < count; index++) {
        if (fprintf(file, "%s\n", lines[index].line) < 0)
            die_errno(path);
    }
    if (fclose(file) != 0)
        die_errno(path);
}

static void append_setting(ConfigLine **lines, size_t *count, size_t *capacity, const char *line) {
    char *copy = strdup(line);
    if (!copy)
        die("out of memory");
    add_line(lines, count, capacity, config_key(copy), copy, 0);
}

static void force_line(ConfigLine **lines, size_t *count, size_t *capacity, const char *key, const char *text) {
    size_t index;
    for (index = 0; index < *count; index++) {
        if (!(*lines)[index].key || strcmp((*lines)[index].key, key) != 0)
            continue;
        free((*lines)[index].line);
        (*lines)[index].line = strdup(text);
        if (!(*lines)[index].line)
            die("out of memory");
        return;
    }
    append_setting(lines, count, capacity, text);
}

/* Whether `line` is the mark after the plan's fields. */
static int at_config(const char *line) {
    return strncmp(line, plan_mark_Config, strlen(plan_mark_Config)) == 0;
}

static const char *field(const char *plan, const char *name, char *out, size_t out_cap) {
    size_t name_len = strlen(name);
    const char *cursor = plan;
    while (*cursor) {
        const char *end = strchr(cursor, '\n');
        size_t length = end ? (size_t)(end - cursor) : strlen(cursor);
        if (length > name_len && strncmp(cursor, name, name_len) == 0 && cursor[name_len] == '\t') {
            size_t value_len = length - name_len - 1;
            if (value_len >= out_cap)
                die("a launch field does not fit");
            memcpy(out, cursor + name_len + 1, value_len);
            out[value_len] = '\0';
            return out;
        }
        if (!end)
            break;
        cursor = end + 1;
    }
    out[0] = '\0';
    return NULL;
}

static void collect_managed(const char *plan, char managed[][128], size_t *count) {
    const char *cursor = plan;
    *count = 0;
    while (*cursor && !at_config(cursor)) {
        const char *end = strchr(cursor, '\n');
        size_t length = end ? (size_t)(end - cursor) : strlen(cursor);
        size_t field_len = strlen(plan_Managed);
        if (length > field_len + 1 && strncmp(cursor, plan_Managed, field_len) == 0
            && cursor[field_len] == '\t') {
            size_t name_len = length - field_len - 1;
            if (*count >= MANAGED_CAP || name_len >= 128)
                die("too many managed directories");
            memcpy(managed[*count], cursor + field_len + 1, name_len);
            managed[*count][name_len] = '\0';
            (*count)++;
        }
        if (!end)
            break;
        cursor = end + 1;
    }
}

/* Every setting that the player changes in the game's menu, with the value
 * the player chose, or the default in the export until they choose one. */
static void apply_player_settings(ConfigLine **lines, size_t *count, size_t *capacity,
                                  const char *plan, const char *data_dir) {
    const char *cursor = plan;
    while (*cursor && !at_config(cursor)) {
        const char *end = strchr(cursor, '\n');
        size_t length = end ? (size_t)(end - cursor) : strlen(cursor);
        char plan_line[LINE_CAP];
        char key[128];
        char line[LINE_CAP];
        int found;
        if (length >= sizeof plan_line)
            die("a launch plan line does not fit");
        memcpy(plan_line, cursor, length);
        plan_line[length] = '\0';
        found = rominabox_player_setting(data_dir, plan_line, key, sizeof key, line, sizeof line);
        if (found < 0)
            die_errno("a player setting in the launch plan is malformed");
        if (found > 0)
            force_line(lines, count, capacity, key, line);
        if (!end)
            break;
        cursor = end + 1;
    }
}

/* A content path that starts at a root, or climbs out with `..`, is not a
 * file inside the app. */
static int path_has_dotdot(const char *path) {
    const char *cursor = path;
    if (is_separator(path[0]))
        return 1;
#if defined(_WIN32)
    /* A drive letter, C:, also points to a place outside the app. */
    if (strchr(path, ':'))
        return 1;
#endif
    while (*cursor) {
        if ((cursor == path || is_separator(cursor[-1])) && starts_with(cursor, "..") &&
            (cursor[2] == '\0' || is_separator(cursor[2])))
            return 1;
        cursor++;
    }
    return 0;
}

static void first_line(const char *path, char *out, size_t out_cap) {
    FILE *file = fs_open(path, "r");
    if (!file) {
        out[0] = '\0';
        return;
    }
    if (!fgets(out, (int)out_cap, file))
        out[0] = '\0';
    fclose(file);
    {
        size_t length = strlen(out);
        while (length > 0 && (out[length - 1] == '\n' || out[length - 1] == '\r'))
            out[--length] = '\0';
    }
}

/* Quiet unless a person started the game, or sound is turned on in the
 * environment. Quiet is opt-out, so a run from a harness is quiet even when
 * nothing is set. ROMINABOX_QUIET makes even a person's launch quiet. */
int rominabox_launch_is_quiet(int opened_by_person, const char *quiet, const char *sound) {
    if (quiet && quiet[0])
        return 1;
    if (sound && sound[0])
        return 0;
    return !opened_by_person;
}

static void set_variable(Launch *launch, const char *name, const char *value) {
    if (launch->variable_count >= LAUNCH_VARIABLES_CAP)
        die("too many launch variables");
    launch->variables[launch->variable_count].name = name;
    launch->variables[launch->variable_count].value = value ? strdup(value) : NULL;
    if (value && !launch->variables[launch->variable_count].value)
        die("out of memory");
    launch->variable_count++;
}

static void add_argument(Launch *launch, const char *argument) {
    if (launch->argument_count >= LAUNCH_ARGUMENTS_CAP - 1)
        die("too many launch arguments");
    launch->arguments[launch->argument_count] = strdup(argument);
    if (!launch->arguments[launch->argument_count])
        die("out of memory");
    launch->argument_count++;
    launch->arguments[launch->argument_count] = NULL;
}

static char *read_plan(const char *resources) {
    char plan_path[PATH_CAP];
    char *plan;
    join_path(plan_path, sizeof plan_path, resources, app_Plan);
    plan = read_file(plan_path, NULL);
    if (!plan)
        die("the game is missing its launch plan");
    return plan;
}

static void read_game_from(const char *plan, LaunchGame *game) {
    char achievements[8] = "0";
    char sandbox[8] = "0";
    memset(game, 0, sizeof *game);
    if (!field(plan, plan_Identity, game->identity, sizeof game->identity) || strchr(game->identity, '/')
        || game->identity[0] == '\0')
        die("the launch plan has no identity");
    if (!field(plan, plan_Title, game->title, sizeof game->title))
        game->title[0] = '\0';
    field(plan, plan_Achievements, achievements, sizeof achievements);
    game->achievements = strcmp(achievements, "1") == 0;
    field(plan, plan_Sandbox, sandbox, sizeof sandbox);
    game->sandbox = strcmp(sandbox, "1") == 0;
    field(plan, plan_AccountsDir, game->accounts_name, sizeof game->accounts_name);
    if (!field(plan, plan_DataDir, game->data_template, sizeof game->data_template))
        die("the launch plan has no data directory");
}

void rominabox_read_game(const char *resources, LaunchGame *game) {
    char *plan = read_plan(resources);
    read_game_from(plan, game);
    free(plan);
}

void rominabox_game_data_folder(const LaunchGame *game, const char *user_data, char *out, size_t out_cap) {
    if (!user_data || !fs_is_absolute(user_data))
        die("there is no per-user folder to keep this game's files in");
    if (starts_with(game->data_template, token_UserData)) {
        int wrote = snprintf(out, out_cap, "%s%s", user_data, game->data_template + strlen(token_UserData));
        if (wrote < 0 || (size_t)wrote >= out_cap)
            die("the data directory does not fit");
    } else if (fs_is_absolute(game->data_template)) {
        snprintf(out, out_cap, "%s", game->data_template);
    } else {
        die("the data directory is not absolute");
    }
    fs_native_path(out);
    if (!fs_is_absolute(out))
        die("the data directory is not absolute");
}

typedef struct {
    const char *from_dir;
    const char *to_dir;
} Copying;

static void copy_tree(const char *from_dir, const char *to_dir);

static int copy_entry(const char *name, void *context) {
    const Copying *copying = context;
    char from[PATH_CAP];
    char to[PATH_CAP];
    join_path(from, sizeof from, copying->from_dir, name);
    join_path(to, sizeof to, copying->to_dir, name);
    /* We leave a link in place, because we follow no link in either test. */
    if (fs_is_directory(from))
        copy_tree(from, to);
    else if (fs_is_file(from) && fs_copy_new(from, to) != 0)
        die_errno(from);
    return 0;
}

static int nothing(const char *name, void *context) {
    (void)name;
    (void)context;
    return 0;
}

/* Copy everything in `from_dir` that is not in `to_dir` yet. We skip a
 * folder that we cannot list, and leave its copy alone. */
static void copy_tree(const char *from_dir, const char *to_dir) {
    Copying copying = {from_dir, to_dir};
    if (fs_list_all(from_dir, nothing, NULL) != 0)
        return;
    mkdir_p(to_dir);
    fs_list_all(from_dir, copy_entry, &copying);
}

/* A game exported in the older layout kept its data in the per-user folder
 * outside the sandbox. On the first launch in the sandbox we copy what that
 * folder contains, and never again once the game has its own config. */
static void bring_previous_saves(const LaunchGame *game, const char *previous_user_data, const char *data_dir) {
    char old_dir[PATH_CAP];
    char marker[PATH_CAP];
    rominabox_game_data_folder(game, previous_user_data, old_dir, sizeof old_dir);
    if (strcmp(old_dir, data_dir) == 0)
        return;
    join_path(marker, sizeof marker, data_dir, "retroarch.cfg");
    if (fs_exists(marker) || !fs_exists(old_dir))
        return;
    copy_tree(old_dir, data_dir);
}

void rominabox_prepare_launch(const LaunchPlaces *places, Launch *launch) {
    const char *resources = places->resources;
    char core_path[PATH_CAP];
    char content_path[PATH_CAP];
    char assets[PATH_CAP];
    char controls_defaults[PATH_CAP];
    char controls_override[PATH_CAP];
    char shader_choice[PATH_CAP];
    char shader_preset[PATH_CAP];
    char content[PATH_CAP];
    char start_at_menu[8];
    char advanced[8];
    char shader_initial[PATH_CAP];
    char managed[MANAGED_CAP][128];
    LaunchGame game;
    char *plan;
    const char *config_text;
    char *data_dir = launch->data_dir;
    size_t managed_count = 0;
    size_t index;
    ConfigLine *lines = NULL;
    size_t line_count = 0;
    size_t line_capacity = 0;

    memset(launch, 0, sizeof *launch);
    plan = read_plan(resources);
    {
        char mark[64];
        snprintf(mark, sizeof mark, "\n%s\n", plan_mark_Config);
        config_text = strstr(plan, mark);
        if (!config_text)
            die("the launch plan has no config");
        config_text += strlen(mark);
    }

    read_game_from(plan, &game);
    if (!field(plan, plan_Content, content, sizeof content) || path_has_dotdot(content))
        die("the launch plan has no content path");
    field(plan, plan_StartAtMenu, start_at_menu, sizeof start_at_menu);
    field(plan, plan_Advanced, advanced, sizeof advanced);
    field(plan, plan_ShaderInitial, shader_initial, sizeof shader_initial);
    collect_managed(plan, managed, &managed_count);

    rominabox_game_data_folder(&game, places->user_data, data_dir, PATH_CAP);

    if (places->before_data_folder)
        places->before_data_folder(data_dir);
    if (places->previous_user_data)
        bring_previous_saves(&game, places->previous_user_data, data_dir);
    mkdir_p(data_dir);
    for (index = 0; index < managed_count; index++) {
        char directory[PATH_CAP];
        join_path(directory, sizeof directory, data_dir, managed[index]);
        mkdir_p(directory);
    }

    /* We apply what the export ships on every launch, and a file left in the
     * game's data by an earlier export or location never overrides it. In
     * applied/ we record what we applied, so we can tell a player's own later
     * change from a stale value. */
    {
        static const Shipped settings[] = {
#define RIB_SHIPPED_SETTINGS(name, app, data) {app, data},
#include "launch_contract.inc"
        };
        static const Shipped files[] = {
#define RIB_SHIPPED_FILES(name, app, data) {app, data},
#include "launch_contract.inc"
        };
        char from[PATH_CAP];
        char to[PATH_CAP];
        char applied_root[PATH_CAP];
        char applied[PATH_CAP];
        char listed[PATH_CAP];
        char failed[PATH_CAP];
        size_t which;
        join_path(applied_root, sizeof applied_root, data_dir, "applied");
        mkdir_p(applied_root);
        for (which = 0; which < sizeof settings / sizeof settings[0]; which++) {
            join_path(from, sizeof from, resources, settings[which].app);
            join_path(to, sizeof to, data_dir, settings[which].data);
            join_path(applied, sizeof applied, applied_root, settings[which].data);
            if (rominabox_apply_shipped_settings(from, to, applied, failed, sizeof failed) != 0)
                die_errno(failed);
        }
        for (which = 0; which < sizeof files / sizeof files[0]; which++) {
            join_path(from, sizeof from, resources, files[which].app);
            join_path(to, sizeof to, data_dir, files[which].data);
            if (snprintf(listed, sizeof listed, "%s.list", files[which].data) >= (int)sizeof listed)
                die("a path does not fit");
            join_path(applied, sizeof applied, applied_root, listed);
            if (rominabox_replace_shipped_files(from, to, applied, failed, sizeof failed) != 0)
                die_errno(failed);
        }
    }

    load_base(&lines, &line_count, &line_capacity, config_text, data_dir, resources);
    join_path(assets, sizeof assets, resources, app_MenuAssets);
    join_path(controls_defaults, sizeof controls_defaults, assets, menu_ControlsDefaults);
    join_path(controls_override, sizeof controls_override, data_dir, menu_data_Controls);
    apply_player_file(&lines, &line_count, &line_capacity, controls_defaults);
    apply_player_file(&lines, &line_count, &line_capacity, controls_override);
    apply_player_settings(&lines, &line_count, &line_capacity, plan, data_dir);
    /* Quiet is opt-out. We publish ROMINABOX_QUIET so that we can apply the
     * same setting in the fork. */
    launch->quiet = rominabox_launch_is_quiet(
        places->opened_by_person, getenv(RIB_ENV_QUIET), getenv(ROMINABOX_SOUND_ENV));
    if (launch->quiet)
        set_variable(launch, RIB_ENV_QUIET, "1");
    /* The window of a quiet run is never in front, and we take a screenshot
     * with the window unfocused. With pause_nonactive on, the console would
     * pause, so the run would never reach its frame limit, or the picture
     * would show a stopped game. We turn it off here for this run, after the
     * player's settings, and leave the player's file unchanged. */
    {
        const char *shot = getenv(RIB_ENV_MENU_SHOT);
        if ((shot && shot[0]) || launch->quiet)
            force_line(
                &lines,
                &line_count,
                &line_capacity,
                "pause_nonactive",
                "pause_nonactive = \"false\""
            );
    }
    /* In a run driven by a menu script, the screens follow the script whatever
     * pads the host has, so we read no controller in such a run. Otherwise the
     * bindings of a connected pad would appear on the Controls screen. We set
     * this after the player files, so a controls.cfg cannot enable one again. */
    if (getenv(RIB_ENV_MENU_SCRIPT))
        force_line(&lines, &line_count, &line_capacity,
            "input_joypad_driver", "input_joypad_driver = \"null\"");
    /* After the player files, so a controls.cfg cannot turn sound back on for
     * this launch. With audio_enable false, no audio driver is ever opened.
     * We replace the frozen driver line with null so the written config
     * contains no device, and do not write this into the player's file. */
    if (launch->quiet) {
        force_line(&lines, &line_count, &line_capacity,
            "audio_driver", "audio_driver = \"null\"");
        force_line(&lines, &line_count, &line_capacity,
            "audio_enable", "audio_enable = \"false\"");
        force_line(&lines, &line_count, &line_capacity,
            "audio_enable_menu", "audio_enable_menu = \"false\"");
        force_line(&lines, &line_count, &line_capacity,
            "audio_enable_menu_ok", "audio_enable_menu_ok = \"false\"");
        force_line(&lines, &line_count, &line_capacity,
            "audio_enable_menu_cancel", "audio_enable_menu_cancel = \"false\"");
        force_line(&lines, &line_count, &line_capacity,
            "audio_enable_menu_scroll", "audio_enable_menu_scroll = \"false\"");
        force_line(&lines, &line_count, &line_capacity,
            "audio_enable_menu_bgm", "audio_enable_menu_bgm = \"false\"");
        force_line(&lines, &line_count, &line_capacity,
            "audio_enable_menu_notice", "audio_enable_menu_notice = \"false\"");
    }

    shader_preset[0] = '\0';
    join_path(shader_choice, sizeof shader_choice, data_dir, menu_data_ShaderChoice);
    if (fs_exists(shader_choice))
        first_line(shader_choice, shader_preset, sizeof shader_preset);
    else if (shader_initial[0])
        join_path(shader_preset, sizeof shader_preset, assets, shader_initial);
    if (shader_preset[0])
        append_setting(&lines, &line_count, &line_capacity, "video_shader_enable = \"true\"");

    join_path(launch->config_path, sizeof launch->config_path, data_dir, "retroarch.cfg");
    write_config(launch->config_path, lines, line_count);
    /* In the quiet check we run this binary to read the config from this run.
     * Going on would start the game, and without the switch the check would
     * open an audio device. Stop once the file is on disk, before we touch
     * anything outside the game's data. */
    if (getenv("ROMINABOX_PLAN_ONLY"))
        _Exit(0);
    join_path(launch->log_path, sizeof launch->log_path, data_dir, "logs/launch.log");

    set_variable(launch, RIB_ENV_ACHIEVEMENTS, game.achievements ? "1" : "0");
    set_variable(launch, RIB_ENV_DATA_DIR, data_dir);
    set_variable(launch, RIB_ENV_GAME_IDENTITY, game.identity);
    /* The folder for QUICK SIGN IN, only when the export lists one. It is in
     * the real per-user application data, not in a sandbox's HOME. When a game
     * cannot reach it, the player plays on without QUICK SIGN IN. */
    {
        char accounts[PATH_CAP];
        int found = game.achievements && game.accounts_name[0];
        if (found && places->accounts_root
            && rominabox_accounts_folder(places->accounts_root, game.accounts_name, accounts, sizeof accounts) == 0)
            set_variable(launch, RIB_ENV_ACCOUNTS_DIR, accounts);
        else {
            set_variable(launch, RIB_ENV_ACCOUNTS_DIR, NULL);
            if (found)
                fprintf(stderr, "ROM-in-a-Box: QUICK SIGN IN is unavailable: %s\n", strerror(errno));
        }
    }
    set_variable(launch, RIB_ENV_TITLE, game.title);
    set_variable(launch, RIB_ENV_RML_ASSETS, assets);
    set_variable(launch, RIB_ENV_ADVANCED_ACCESS, strcmp(advanced, "1") == 0 ? "1" : "0");
    set_variable(launch, RIB_ENV_START_AT_MENU, strcmp(start_at_menu, "1") == 0 ? "1" : NULL);
    set_variable(launch, "LIBRETRO_SYSTEM_DIRECTORY", NULL);
    set_variable(launch, "LIBRETRO_DIRECTORY", NULL);
    set_variable(launch, "LIBRETRO_ASSETS_DIRECTORY", NULL);
    set_variable(launch, "LIBRETRO_AUTOCONFIG_DIRECTORY", NULL);
    set_variable(launch, "LIBRETRO_CHEATS_DIRECTORY", NULL);
    set_variable(launch, "LIBRETRO_DATABASE_DIRECTORY", NULL);
    set_variable(launch, "LIBRETRO_VIDEO_FILTER_DIRECTORY", NULL);
    set_variable(launch, "LIBRETRO_VIDEO_SHADER_DIRECTORY", NULL);

    join_path(core_path, sizeof core_path, resources, core_file());
    join_path(content_path, sizeof content_path, resources, content);
    add_argument(launch, "--config");
    add_argument(launch, launch->config_path);
    add_argument(launch, "--libretro");
    add_argument(launch, core_path);
    add_argument(launch, content_path);
    if (shader_preset[0]) {
        add_argument(launch, "--set-shader");
        add_argument(launch, shader_preset);
    }
    {
        const char *verbose = getenv("ROMINABOX_VERBOSE");
        const char *frames = getenv("ROMINABOX_MAX_FRAMES");
        if (verbose && strcmp(verbose, "1") == 0)
            add_argument(launch, "--verbose");
        if (frames && frames[0]) {
            char frames_argument[32];
            const char *digit = frames;
            while (*digit >= '0' && *digit <= '9')
                digit++;
            if (*digit != '\0' || strlen(frames) > 6)
                die("ROMINABOX_MAX_FRAMES is not a frame count");
            snprintf(frames_argument, sizeof frames_argument, "--max-frames=%s", frames);
            add_argument(launch, frames_argument);
        }
    }
}
