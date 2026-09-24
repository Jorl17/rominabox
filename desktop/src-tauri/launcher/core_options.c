#include "core_options.h"

#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define OPTIONS_PATH_CAP 4096

typedef struct {
    char *key; /* NULL for a line that sets nothing */
    char *value;
    char *text;
} OptionLine;

typedef struct {
    OptionLine *lines;
    size_t count;
    size_t capacity;
    char *raw; /* the file as read; NULL when there is no file */
} OptionFile;

static char *copy_span(const char *start, size_t length) {
    char *copy = malloc(length + 1);
    if (!copy)
        return NULL;
    memcpy(copy, start, length);
    copy[length] = '\0';
    return copy;
}

static void line_free(OptionLine *line) {
    free(line->key);
    free(line->value);
    free(line->text);
}

static void options_free(OptionFile *file) {
    size_t index;
    for (index = 0; index < file->count; index++)
        line_free(&file->lines[index]);
    free(file->lines);
    free(file->raw);
    memset(file, 0, sizeof *file);
}

static int options_append(OptionFile *file, char *key, char *value, char *text) {
    if (file->count == file->capacity) {
        size_t capacity = file->capacity ? file->capacity * 2 : 32;
        OptionLine *lines = realloc(file->lines, capacity * sizeof *lines);
        if (!lines)
            return -1;
        file->lines = lines;
        file->capacity = capacity;
    }
    file->lines[file->count].key = key;
    file->lines[file->count].value = value;
    file->lines[file->count].text = text;
    file->count++;
    return 0;
}

static int is_blank(char c) {
    return c == ' ' || c == '\t';
}

/* key = "value", the form RetroArch uses. We keep a line in any other form
 * as text, and it sets nothing. */
static int parse_line(const char *start, const char *end, char **key, char **value) {
    const char *key_end;
    const char *value_start;
    const char *value_end;
    *key = NULL;
    *value = NULL;
    while (start < end && is_blank(*start))
        start++;
    if (start == end || *start == '#')
        return 0;
    key_end = start;
    while (key_end < end && !is_blank(*key_end) && *key_end != '=')
        key_end++;
    value_start = key_end;
    while (value_start < end && is_blank(*value_start))
        value_start++;
    if (key_end == start || value_start == end || *value_start != '=')
        return 0;
    value_start++;
    while (value_start < end && is_blank(*value_start))
        value_start++;
    if (value_start < end && *value_start == '"') {
        value_start++;
        value_end = memchr(value_start, '"', (size_t)(end - value_start));
        if (!value_end)
            value_end = end;
    } else {
        value_end = end;
        while (value_end > value_start && is_blank(value_end[-1]))
            value_end--;
    }
    *key = copy_span(start, (size_t)(key_end - start));
    *value = copy_span(value_start, (size_t)(value_end - value_start));
    if (*key && *value)
        return 0;
    free(*key);
    free(*value);
    *key = NULL;
    *value = NULL;
    return -1;
}

/* A missing file reads as an empty one. */
static int options_read(const char *path, OptionFile *file) {
    FILE *stream;
    long size;
    const char *cursor;
    memset(file, 0, sizeof *file);
    stream = fopen(path, "rb");
    if (!stream)
        return errno == ENOENT ? 0 : -1;
    if (fseek(stream, 0, SEEK_END) != 0 || (size = ftell(stream)) < 0 || fseek(stream, 0, SEEK_SET) != 0) {
        fclose(stream);
        return -1;
    }
    file->raw = malloc((size_t)size + 1);
    if (!file->raw || fread(file->raw, 1, (size_t)size, stream) != (size_t)size) {
        fclose(stream);
        options_free(file);
        errno = EIO;
        return -1;
    }
    fclose(stream);
    file->raw[size] = '\0';
    for (cursor = file->raw; *cursor;) {
        const char *newline = strchr(cursor, '\n');
        const char *end = newline ? newline : cursor + strlen(cursor);
        char *key;
        char *value;
        char *text;
        if (end > cursor && end[-1] == '\r')
            end--;
        text = copy_span(cursor, (size_t)(end - cursor));
        if (!text || parse_line(cursor, end, &key, &value) != 0
            || options_append(file, key, value, text) != 0) {
            free(text);
            options_free(file);
            errno = ENOMEM;
            return -1;
        }
        if (!newline)
            break;
        cursor = newline + 1;
    }
    return 0;
}

static const char *options_get(const OptionFile *file, const char *key) {
    size_t index;
    for (index = 0; index < file->count; index++) {
        if (file->lines[index].key && strcmp(file->lines[index].key, key) == 0)
            return file->lines[index].value;
    }
    return NULL;
}

static int options_set(OptionFile *file, const char *key, const char *value) {
    size_t index;
    size_t length = strlen(key) + strlen(value) + sizeof " = \"\"";
    int found = 0;
    for (index = 0; index < file->count; index++) {
        OptionLine *line = &file->lines[index];
        char *text;
        char *copy;
        if (!line->key || strcmp(line->key, key) != 0)
            continue;
        text = malloc(length);
        copy = strdup(value);
        if (!text || !copy) {
            free(text);
            free(copy);
            return -1;
        }
        snprintf(text, length, "%s = \"%s\"", key, value);
        free(line->text);
        free(line->value);
        line->text = text;
        line->value = copy;
        found = 1;
    }
    if (!found) {
        char *text = malloc(length);
        char *key_copy = strdup(key);
        char *value_copy = strdup(value);
        if (!text || !key_copy || !value_copy
            || (snprintf(text, length, "%s = \"%s\"", key, value),
                options_append(file, key_copy, value_copy, text) != 0)) {
            free(text);
            free(key_copy);
            free(value_copy);
            return -1;
        }
    }
    return 0;
}

static void options_remove(OptionFile *file, const char *key) {
    size_t from;
    size_t to = 0;
    for (from = 0; from < file->count; from++) {
        OptionLine *line = &file->lines[from];
        if (line->key && strcmp(line->key, key) == 0) {
            line_free(line);
            continue;
        }
        file->lines[to++] = *line;
    }
    file->count = to;
}

/* We write beside the file and rename over it, so that if the player is
 * stopped halfway, the file is never half written. */
static int write_text(const char *path, const char *text, size_t length) {
    char temporary[OPTIONS_PATH_CAP];
    FILE *stream;
    int wrote = snprintf(temporary, sizeof temporary, "%s.rominabox-new", path);
    if (wrote < 0 || (size_t)wrote >= sizeof temporary) {
        errno = ENAMETOOLONG;
        return -1;
    }
    stream = fopen(temporary, "wb");
    if (!stream)
        return -1;
    if (fwrite(text, 1, length, stream) != length) {
        fclose(stream);
        unlink(temporary);
        errno = EIO;
        return -1;
    }
    if (fclose(stream) != 0 || rename(temporary, path) != 0) {
        int saved = errno;
        unlink(temporary);
        errno = saved;
        return -1;
    }
    return 0;
}

static int options_write(const OptionFile *file, const char *path) {
    size_t length = 0;
    size_t index;
    char *text;
    char *cursor;
    int result;
    for (index = 0; index < file->count; index++)
        length += strlen(file->lines[index].text) + 1;
    text = malloc(length + 1);
    if (!text)
        return -1;
    cursor = text;
    for (index = 0; index < file->count; index++) {
        size_t line_length = strlen(file->lines[index].text);
        memcpy(cursor, file->lines[index].text, line_length);
        cursor += line_length;
        *cursor++ = '\n';
    }
    result = write_text(path, text, length);
    free(text);
    return result;
}

static int join(char *out, size_t cap, const char *left, const char *right) {
    int wrote = snprintf(out, cap, "%s/%s", left, right);
    if (wrote < 0 || (size_t)wrote >= cap) {
        errno = ENAMETOOLONG;
        return -1;
    }
    return 0;
}

static int make_directory(const char *path) {
    return mkdir(path, 0755) == 0 || errno == EEXIST ? 0 : -1;
}

static int fail(const char *path, char *failed, size_t failed_cap) {
    int saved = errno;
    if (failed && failed_cap)
        snprintf(failed, failed_cap, "%s", path);
    errno = saved;
    return -1;
}

static int apply_file(
    const char *shipped_path,
    const char *game_root,
    const char *game_dir,
    const char *game_path,
    const char *applied_root,
    const char *applied_dir,
    const char *applied_path,
    char *failed,
    size_t failed_cap) {
    OptionFile shipped;
    OptionFile game;
    OptionFile applied;
    const char *where = shipped_path;
    int changed = 0;
    int result = -1;
    size_t index;
    memset(&game, 0, sizeof game);
    memset(&applied, 0, sizeof applied);
    if (options_read(shipped_path, &shipped) != 0)
        goto done;
    if (!shipped.raw) {
        errno = ENOENT;
        goto done;
    }
    where = game_path;
    if (options_read(game_path, &game) != 0)
        goto done;
    where = applied_path;
    if (options_read(applied_path, &applied) != 0)
        goto done;

    for (index = 0; index < shipped.count; index++) {
        const OptionLine *want = &shipped.lines[index];
        const char *have;
        const char *last;
        if (!want->key)
            continue;
        have = options_get(&game, want->key);
        last = options_get(&applied, want->key);
        if (have && strcmp(have, want->value) == 0)
            continue;
        /* Changed since we applied it at a launch, so the player chose it. */
        if (have && last && strcmp(have, last) != 0)
            continue;
        where = game_path;
        if (options_set(&game, want->key, want->value) != 0)
            goto done;
        changed = 1;
    }
    for (index = 0; index < applied.count; index++) {
        const OptionLine *old = &applied.lines[index];
        const char *have;
        if (!old->key || options_get(&shipped, old->key))
            continue;
        have = options_get(&game, old->key);
        if (have && strcmp(have, old->value) == 0) {
            options_remove(&game, old->key);
            changed = 1;
        }
    }

    if (changed) {
        where = game_dir;
        if (make_directory(game_root) != 0 || make_directory(game_dir) != 0)
            goto done;
        where = game_path;
        if (options_write(&game, game_path) != 0)
            goto done;
    }
    if (!applied.raw || strcmp(applied.raw, shipped.raw) != 0) {
        where = applied_dir;
        if (make_directory(applied_root) != 0 || make_directory(applied_dir) != 0)
            goto done;
        where = applied_path;
        if (write_text(applied_path, shipped.raw, strlen(shipped.raw)) != 0)
            goto done;
    }
    result = 0;
done:
    if (result != 0)
        fail(where, failed, failed_cap);
    {
        int saved = errno;
        options_free(&shipped);
        options_free(&game);
        options_free(&applied);
        errno = saved;
    }
    return result;
}

int rominabox_apply_core_options(
    const char *shipped,
    const char *game,
    const char *applied,
    char *failed,
    size_t failed_cap) {
    DIR *cores = opendir(shipped);
    struct dirent *core;
    int result = 0;
    if (!cores)
        return errno == ENOENT ? 0 : fail(shipped, failed, failed_cap);
    while (result == 0 && (core = readdir(cores))) {
        char shipped_dir[OPTIONS_PATH_CAP];
        char game_dir[OPTIONS_PATH_CAP];
        char applied_dir[OPTIONS_PATH_CAP];
        struct stat info;
        DIR *files;
        struct dirent *file;
        if (core->d_name[0] == '.')
            continue;
        if (join(shipped_dir, sizeof shipped_dir, shipped, core->d_name) != 0
            || join(game_dir, sizeof game_dir, game, core->d_name) != 0
            || join(applied_dir, sizeof applied_dir, applied, core->d_name) != 0) {
            result = fail(shipped, failed, failed_cap);
            break;
        }
        if (lstat(shipped_dir, &info) != 0 || !S_ISDIR(info.st_mode))
            continue;
        files = opendir(shipped_dir);
        if (!files) {
            result = fail(shipped_dir, failed, failed_cap);
            break;
        }
        while (result == 0 && (file = readdir(files))) {
            char shipped_path[OPTIONS_PATH_CAP];
            char game_path[OPTIONS_PATH_CAP];
            char applied_path[OPTIONS_PATH_CAP];
            if (file->d_name[0] == '.')
                continue;
            if (join(shipped_path, sizeof shipped_path, shipped_dir, file->d_name) != 0
                || join(game_path, sizeof game_path, game_dir, file->d_name) != 0
                || join(applied_path, sizeof applied_path, applied_dir, file->d_name) != 0) {
                result = fail(shipped_dir, failed, failed_cap);
                break;
            }
            if (lstat(shipped_path, &info) != 0 || !S_ISREG(info.st_mode))
                continue;
            result = apply_file(
                shipped_path, game, game_dir, game_path,
                applied, applied_dir, applied_path, failed, failed_cap);
        }
        {
            int saved = errno;
            closedir(files);
            errno = saved;
        }
    }
    {
        int saved = errno;
        closedir(cores);
        errno = saved;
    }
    return result;
}
