#include "player_settings.h"

#include "portable_fs.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>

/* Its plan line, as declared in launch_contract.inc. */
#define RIB_PLAN_FIELD(name, field) static const char plan_##name[] = field;
#include "launch_contract.inc"
#define SETTING_FIELD_CAP 256
#define SETTING_FILE_CAP 4096

static int is_blank(char c) {
    return c == ' ' || c == '\t';
}

/* The next tab-separated field of `*cursor`, copied into `out`. */
static int take_field(const char **cursor, char *out, size_t cap) {
    const char *end = strchr(*cursor, '\t');
    size_t length = end ? (size_t)(end - *cursor) : strlen(*cursor);
    if (length == 0 || length >= cap)
        return -1;
    memcpy(out, *cursor, length);
    out[length] = '\0';
    *cursor += length + (end ? 1 : 0);
    return 0;
}

/* A file directly inside the data directory: no separator, and not a name
 * that means somewhere else. */
static int plain_file_name(const char *name) {
    return name[0] != '.' && !strchr(name, '/') && !strchr(name, '\\') && !strchr(name, ':');
}

static int plain_key(const char *key) {
    const char *cursor;
    for (cursor = key; *cursor; cursor++) {
        char c = *cursor;
        if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_'))
            return 0;
    }
    return 1;
}

/* A value that we can write between quotes in config text. */
static int plain_value(const char *value, size_t length) {
    size_t index;
    for (index = 0; index < length; index++) {
        unsigned char c = (unsigned char)value[index];
        if (c == '"' || c < 0x20)
            return 0;
    }
    return 1;
}

/* The value of `key` in `file`, into `out`, or 0 when it has none. */
static int chosen_value(const char *path, const char *key, char *out, size_t cap) {
    char text[SETTING_FILE_CAP];
    size_t length;
    const char *cursor;
    size_t key_length = strlen(key);
    FILE *file = fs_open(path, "rb");
    if (!file)
        return 0;
    length = fread(text, 1, sizeof text - 1, file);
    fclose(file);
    text[length] = '\0';
    for (cursor = text; *cursor;) {
        const char *end = strpbrk(cursor, "\r\n");
        const char *line_end = end ? end : cursor + strlen(cursor);
        const char *at = cursor;
        cursor = end ? end + 1 : line_end;
        while (at < line_end && is_blank(*at))
            at++;
        if ((size_t)(line_end - at) <= key_length || strncmp(at, key, key_length) != 0)
            continue;
        at += key_length;
        while (at < line_end && is_blank(*at))
            at++;
        if (at == line_end || *at != '=')
            continue;
        at++;
        while (at < line_end && is_blank(*at))
            at++;
        {
            const char *value_end = line_end;
            while (value_end > at && is_blank(value_end[-1]))
                value_end--;
            if (value_end - at >= 2 && *at == '"' && value_end[-1] == '"') {
                at++;
                value_end--;
            }
            if (value_end == at || (size_t)(value_end - at) >= cap || !plain_value(at, (size_t)(value_end - at)))
                return 0;
            memcpy(out, at, (size_t)(value_end - at));
            out[value_end - at] = '\0';
            return 1;
        }
    }
    return 0;
}

int rominabox_player_setting(
    const char *data_dir,
    const char *plan_line,
    char *key,
    size_t key_cap,
    char *line,
    size_t line_cap) {
    char file[SETTING_FIELD_CAP];
    char fallback[SETTING_FIELD_CAP];
    char chosen[SETTING_FIELD_CAP];
    char path[SETTING_FILE_CAP];
    const char *cursor = plan_line;
    const char *value;
    int wrote;

    if (strncmp(plan_line, plan_PlayerSetting, strlen(plan_PlayerSetting)) != 0
        || plan_line[strlen(plan_PlayerSetting)] != '\t')
        return 0;
    cursor += strlen(plan_PlayerSetting) + 1;
    if (take_field(&cursor, file, sizeof file) != 0 || take_field(&cursor, key, key_cap) != 0
        || take_field(&cursor, fallback, sizeof fallback) != 0 || *cursor != '\0'
        || !plain_file_name(file) || !plain_key(key) || !plain_value(fallback, strlen(fallback))) {
        errno = EINVAL;
        return -1;
    }
    wrote = snprintf(path, sizeof path, "%s/%s", data_dir, file);
    if (wrote < 0 || (size_t)wrote >= sizeof path) {
        errno = ENAMETOOLONG;
        return -1;
    }
    value = chosen_value(path, key, chosen, sizeof chosen) ? chosen : fallback;
    wrote = snprintf(line, line_cap, "%s = \"%s\"", key, value);
    if (wrote < 0 || (size_t)wrote >= line_cap) {
        errno = ENAMETOOLONG;
        return -1;
    }
    return 1;
}
