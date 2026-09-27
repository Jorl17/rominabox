#include "shipped_files.h"

#include "portable_fs.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define FILES_PATH_CAP 4096
#define CHUNK 65536

typedef struct {
    char **names;
    size_t count;
    size_t capacity;
} Names;

static void names_free(Names *names) {
    size_t index;
    for (index = 0; index < names->count; index++)
        free(names->names[index]);
    free(names->names);
    memset(names, 0, sizeof *names);
}

static int names_has(const Names *names, const char *name) {
    size_t index;
    for (index = 0; index < names->count; index++)
        if (strcmp(names->names[index], name) == 0)
            return 1;
    return 0;
}

/* A name that stays inside the directory it is joined to. The record is in
 * the game's data, where anything could have written it. */
static int plain_name(const char *name) {
    return name[0] && strcmp(name, ".") != 0 && strcmp(name, "..") != 0
        && !strchr(name, '/') && !strchr(name, '\\') && !strchr(name, ':');
}

static int names_add(Names *names, const char *name, size_t length) {
    char *copy;
    if (names->count == names->capacity) {
        size_t capacity = names->capacity ? names->capacity * 2 : 8;
        char **grown = realloc(names->names, capacity * sizeof *grown);
        if (!grown) {
            errno = ENOMEM;
            return -1;
        }
        names->names = grown;
        names->capacity = capacity;
    }
    copy = malloc(length + 1);
    if (!copy) {
        errno = ENOMEM;
        return -1;
    }
    memcpy(copy, name, length);
    copy[length] = '\0';
    names->names[names->count++] = copy;
    return 0;
}

static int fail(const char *path, char *failed, size_t failed_cap) {
    int saved = errno;
    if (failed && failed_cap)
        snprintf(failed, failed_cap, "%s", path);
    errno = saved;
    return -1;
}

/* A missing record reads as no names. */
static int read_record(const char *path, Names *names) {
    FILE *stream = fs_open(path, "rb");
    char line[FILES_PATH_CAP];
    memset(names, 0, sizeof *names);
    if (!stream)
        return errno == ENOENT ? 0 : -1;
    while (fgets(line, sizeof line, stream)) {
        size_t length = strcspn(line, "\r\n");
        line[length] = '\0';
        if (plain_name(line) && !names_has(names, line) && names_add(names, line, length) != 0) {
            fclose(stream);
            names_free(names);
            return -1;
        }
    }
    fclose(stream);
    return 0;
}

/* 1 when both files hold the same bytes, 0 when they differ or `game` is
 * absent, -1 on a read error. */
static int same_bytes(const char *shipped, const char *game) {
    FILE *left = fs_open(shipped, "rb");
    FILE *right;
    static unsigned char a[CHUNK];
    static unsigned char b[CHUNK];
    int result = 1;
    if (!left)
        return -1;
    right = fs_open(game, "rb");
    if (!right) {
        fclose(left);
        return errno == ENOENT ? 0 : -1;
    }
    for (;;) {
        size_t got_left = fread(a, 1, sizeof a, left);
        size_t got_right = fread(b, 1, sizeof b, right);
        if (ferror(left) || ferror(right)) {
            errno = EIO;
            result = -1;
            break;
        }
        if (got_left != got_right || memcmp(a, b, got_left) != 0) {
            result = 0;
            break;
        }
        if (got_left == 0)
            break;
    }
    fclose(left);
    fclose(right);
    return result;
}

/* `from` onto `to`, through a temporary file beside `to`. */
static int copy_over(const char *from, const char *to) {
    char temporary[FILES_PATH_CAP];
    static unsigned char chunk[CHUNK];
    FILE *source;
    FILE *target;
    int wrote = snprintf(temporary, sizeof temporary, "%s.rominabox-new", to);
    int result = 0;
    if (wrote < 0 || (size_t)wrote >= sizeof temporary) {
        errno = ENAMETOOLONG;
        return -1;
    }
    source = fs_open(from, "rb");
    if (!source)
        return -1;
    target = fs_open(temporary, "wb");
    if (!target) {
        fclose(source);
        return -1;
    }
    for (;;) {
        size_t got = fread(chunk, 1, sizeof chunk, source);
        if (got && fwrite(chunk, 1, got, target) != got) {
            result = -1;
            break;
        }
        if (got < sizeof chunk) {
            if (ferror(source))
                result = -1;
            break;
        }
    }
    fclose(source);
    if (fclose(target) != 0)
        result = -1;
    if (result == 0 && fs_replace(temporary, to) == 0)
        return 0;
    {
        int saved = errno ? errno : EIO;
        fs_remove(temporary);
        errno = saved;
    }
    return -1;
}

typedef struct {
    const char *shipped;
    const char *game;
    Names now;
    char *failed;
    size_t failed_cap;
} Walk;

static int bring_one(const char *name, void *context) {
    Walk *walk = context;
    char shipped_path[FILES_PATH_CAP];
    char game_path[FILES_PATH_CAP];
    int same;
    if (!plain_name(name))
        return 0;
    if (fs_join(shipped_path, sizeof shipped_path, walk->shipped, name) != 0
        || fs_join(game_path, sizeof game_path, walk->game, name) != 0)
        return fail(walk->shipped, walk->failed, walk->failed_cap);
    if (!fs_is_file(shipped_path))
        return 0;
    if (names_add(&walk->now, name, strlen(name)) != 0)
        return fail(shipped_path, walk->failed, walk->failed_cap);
    same = same_bytes(shipped_path, game_path);
    if (same < 0)
        return fail(game_path, walk->failed, walk->failed_cap);
    if (!same && copy_over(shipped_path, game_path) != 0)
        return fail(game_path, walk->failed, walk->failed_cap);
    return 0;
}

/* One name a line, replaced whole. */
static int write_record(const char *path, const Names *names) {
    size_t length = 0, at = 0, index;
    char *text;
    int result;
    for (index = 0; index < names->count; index++)
        length += strlen(names->names[index]) + 1;
    text = malloc(length + 1);
    if (!text) {
        errno = ENOMEM;
        return -1;
    }
    for (index = 0; index < names->count; index++) {
        size_t size = strlen(names->names[index]);
        memcpy(text + at, names->names[index], size);
        at += size;
        text[at++] = '\n';
    }
    result = fs_write_file(path, text, at);
    free(text);
    return result;
}

int rominabox_replace_shipped_files(
    const char *shipped,
    const char *game,
    const char *record,
    char *failed,
    size_t failed_cap) {
    Names before;
    Walk walk;
    size_t index;
    int result = -1;
    memset(&walk, 0, sizeof walk);
    walk.shipped = shipped;
    walk.game = game;
    walk.failed = failed;
    walk.failed_cap = failed_cap;
    if (read_record(record, &before) != 0)
        return fail(record, failed, failed_cap);
    if (fs_exists(shipped)) {
        if (fs_make_directory(game) != 0) {
            fail(game, failed, failed_cap);
            goto done;
        }
        if (fs_list(shipped, bring_one, &walk) != 0) {
            if (!walk.now.count && failed && failed_cap)
                fail(shipped, failed, failed_cap);
            goto done;
        }
    }
    for (index = 0; index < before.count; index++) {
        char game_path[FILES_PATH_CAP];
        if (names_has(&walk.now, before.names[index]))
            continue;
        if (fs_join(game_path, sizeof game_path, game, before.names[index]) != 0
            || fs_remove(game_path) != 0) {
            fail(game_path, failed, failed_cap);
            goto done;
        }
    }
    if (walk.now.count ? write_record(record, &walk.now) != 0 : fs_remove(record) != 0) {
        fail(record, failed, failed_cap);
        goto done;
    }
    result = 0;
done:
    {
        int saved = errno;
        names_free(&before);
        names_free(&walk.now);
        errno = saved;
    }
    return result;
}
