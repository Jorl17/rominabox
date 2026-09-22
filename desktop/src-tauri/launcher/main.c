#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <mach-o/dyld.h>
#include <mach-o/loader.h>
#include <pwd.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define PATH_CAP 4096
#define LINE_CAP 8192
#define MANAGED_CAP 64

static void die(const char *message) {
    fprintf(stderr, "ROM-in-a-Box: %s\n", message);
    exit(1);
}

static void die_errno(const char *message) {
    fprintf(stderr, "ROM-in-a-Box: %s: %s\n", message, strerror(errno));
    exit(1);
}

static int starts_with(const char *value, const char *prefix) {
    return strncmp(value, prefix, strlen(prefix)) == 0;
}

static void join_path(char *out, size_t out_cap, const char *left, const char *right) {
    size_t left_len = strlen(left);
    int need_slash = left_len > 0 && left[left_len - 1] != '/';
    int wrote = snprintf(out, out_cap, "%s%s%s", left, need_slash ? "/" : "", right);
    if (wrote < 0 || (size_t)wrote >= out_cap)
        die("a path does not fit");
}

static void mkdir_one(const char *path) {
    if (mkdir(path, 0755) == 0 || errno == EEXIST)
        return;
    die_errno(path);
}

static void mkdir_p(const char *path) {
    char buffer[PATH_CAP];
    size_t length = strlen(path);
    size_t index;
    if (length == 0 || length >= sizeof buffer)
        die("a directory path does not fit");
    memcpy(buffer, path, length + 1);
    for (index = 1; index < length; index++) {
        if (buffer[index] != '/')
            continue;
        buffer[index] = '\0';
        mkdir_one(buffer);
        buffer[index] = '/';
    }
    mkdir_one(buffer);
}

static char *read_file(const char *path, size_t *length_out) {
    FILE *file = fopen(path, "rb");
    long length;
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

static int copy_file_if_absent(const char *from, const char *to) {
    char buffer[8192];
    int in;
    int out;
    ssize_t count;
    struct stat info;
    if (lstat(to, &info) == 0)
        return 0;
    in = open(from, O_RDONLY | O_NOFOLLOW);
    if (in < 0)
        return -1;
    out = open(to, O_WRONLY | O_CREAT | O_EXCL, 0644);
    if (out < 0) {
        close(in);
        if (errno == EEXIST)
            return 0;
        return -1;
    }
    while ((count = read(in, buffer, sizeof buffer)) > 0) {
        char *cursor = buffer;
        while (count > 0) {
            ssize_t wrote = write(out, cursor, (size_t)count);
            if (wrote < 0) {
                close(in);
                close(out);
                unlink(to);
                return -1;
            }
            cursor += wrote;
            count -= wrote;
        }
    }
    close(in);
    if (close(out) != 0) {
        unlink(to);
        return -1;
    }
    return count < 0 ? -1 : 0;
}

static void seed_files(const char *from_dir, const char *to_dir) {
    DIR *directory = opendir(from_dir);
    struct dirent *entry;
    if (!directory)
        return;
    mkdir_p(to_dir);
    while ((entry = readdir(directory))) {
        char from[PATH_CAP];
        char to[PATH_CAP];
        struct stat info;
        if (entry->d_name[0] == '.')
            continue;
        join_path(from, sizeof from, from_dir, entry->d_name);
        if (lstat(from, &info) != 0 || !S_ISREG(info.st_mode))
            continue;
        join_path(to, sizeof to, to_dir, entry->d_name);
        if (copy_file_if_absent(from, to) != 0)
            die_errno(from);
    }
    closedir(directory);
}

static void seed_nested(const char *from_dir, const char *to_dir) {
    DIR *directory = opendir(from_dir);
    struct dirent *entry;
    if (!directory)
        return;
    while ((entry = readdir(directory))) {
        char from[PATH_CAP];
        char to[PATH_CAP];
        struct stat info;
        if (entry->d_name[0] == '.')
            continue;
        join_path(from, sizeof from, from_dir, entry->d_name);
        if (lstat(from, &info) != 0 || !S_ISDIR(info.st_mode))
            continue;
        join_path(to, sizeof to, to_dir, entry->d_name);
        seed_files(from, to);
    }
    closedir(directory);
}

static void copy_tree(const char *from_dir, const char *to_dir) {
    DIR *directory = opendir(from_dir);
    struct dirent *entry;
    if (!directory)
        return;
    mkdir_p(to_dir);
    while ((entry = readdir(directory))) {
        char from[PATH_CAP];
        char to[PATH_CAP];
        struct stat info;
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0)
            continue;
        join_path(from, sizeof from, from_dir, entry->d_name);
        if (lstat(from, &info) != 0)
            continue;
        if (S_ISLNK(info.st_mode))
            continue;
        join_path(to, sizeof to, to_dir, entry->d_name);
        if (S_ISDIR(info.st_mode))
            copy_tree(from, to);
        else if (S_ISREG(info.st_mode) && copy_file_if_absent(from, to) != 0)
            die_errno(from);
    }
    closedir(directory);
}

typedef struct {
    char *key;
    char *line;
    int frozen;
} ConfigLine;

static int allowed_player_key(const char *key) {
    if (starts_with(key, "input_player"))
        return 1;
    if (starts_with(key, "rib_label_"))
        return 1;
    if (starts_with(key, "controls_"))
        return 1;
    if (strcmp(key, "audio_volume") == 0)
        return 1;
    if (strcmp(key, "audio_mute_enable") == 0)
        return 1;
    /* pause_nonactive is the author's frozen choice, and a player file does
     * not replace it. In a screenshot run we set it after we apply the files. */
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

static void load_base(ConfigLine **lines, size_t *count, size_t *capacity, const char *text, const char *data_dir, const char *bundle_dir) {
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
        replace_token(line, "$bundle_dir", bundle_dir);
        replace_token(line, "$data_dir", data_dir);
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
    FILE *file = fopen(path, "r");
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

static void write_config(const char *path, ConfigLine *lines, size_t count) {
    FILE *file = fopen(path, "w");
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
    while (*cursor && strncmp(cursor, "---config---", 12) != 0) {
        const char *end = strchr(cursor, '\n');
        size_t length = end ? (size_t)(end - cursor) : strlen(cursor);
        if (length > 8 && strncmp(cursor, "managed\t", 8) == 0) {
            size_t name_len = length - 8;
            if (*count >= MANAGED_CAP || name_len >= 128)
                die("too many managed directories");
            memcpy(managed[*count], cursor + 8, name_len);
            managed[*count][name_len] = '\0';
            (*count)++;
        }
        if (!end)
            break;
        cursor = end + 1;
    }
}

static int path_has_dotdot(const char *path) {
    const char *cursor = path;
    if (path[0] == '/')
        return 1;
    while (*cursor) {
        if ((cursor == path || cursor[-1] == '/') && starts_with(cursor, "..") &&
            (cursor[2] == '\0' || cursor[2] == '/'))
            return 1;
        cursor++;
    }
    return 0;
}

static void first_line(const char *path, char *out, size_t out_cap) {
    FILE *file = fopen(path, "r");
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

static int writes_the_account_support_directory(const char *data_dir) {
    struct passwd *user = getpwuid(getuid());
    char prefix[PATH_CAP];
    int wrote;
    if (!user || !user->pw_dir || user->pw_dir[0] != '/')
        return 0;
    wrote = snprintf(
        prefix,
        sizeof prefix,
        "%s/Library/Application Support/ROM-in-a-Box/",
        user->pw_dir
    );
    if (wrote < 0 || (size_t)wrote >= sizeof prefix)
        return 0;
    return starts_with(data_dir, prefix);
}

static void migrate_previous_saves(const char *data_dir) {
    struct passwd *user = getpwuid(getuid());
    const char *home = getenv("HOME");
    char old_dir[PATH_CAP];
    char marker[PATH_CAP];
    char real_old[PATH_CAP];
    char real_new[PATH_CAP];
    const char *suffix;
    if (!user || !user->pw_dir || !home || home[0] != '/')
        return;
    if (!starts_with(data_dir, home) || data_dir[strlen(home)] != '/')
        return;
    suffix = data_dir + strlen(home);
    if (snprintf(old_dir, sizeof old_dir, "%s%s", user->pw_dir, suffix) >= (int)sizeof old_dir)
        return;
    if (realpath(old_dir, real_old) && realpath(data_dir, real_new) && strcmp(real_old, real_new) == 0)
        return;
    join_path(marker, sizeof marker, data_dir, "retroarch.cfg");
    if (access(marker, F_OK) == 0 || access(old_dir, F_OK) != 0)
        return;
    copy_tree(old_dir, data_dir);
}

static char *forwarded_argv[12];
static int forwarded_argc;

static void publish_arguments(void) {
    const struct mach_header_64 *header =
        (const struct mach_header_64 *)_dyld_get_image_header(0);
    const uint8_t *commands;
    uint32_t offset = 0;
    uint32_t command_index;
    if (!header || header->magic != MH_MAGIC_64)
        return;
    commands = (const uint8_t *)(header + 1);
    for (command_index = 0; command_index < header->ncmds; command_index++) {
        const struct load_command *command =
            (const struct load_command *)(commands + offset);
        if (command->cmd == LC_MAIN) {
            const struct entry_point_command *entry =
                (const struct entry_point_command *)command;
            const uint8_t *trampoline = (const uint8_t *)header + entry->entryoff;
            uint64_t argc_address;
            uint64_t argv_address;
            intptr_t slide;
            if (memcmp(trampoline + 20, "RBOXLNCH", 8) != 0)
                return;
            memcpy(&argc_address, trampoline + 28, sizeof argc_address);
            memcpy(&argv_address, trampoline + 36, sizeof argv_address);
            slide = _dyld_get_image_vmaddr_slide(0);
            *(uint64_t *)(slide + (intptr_t)argc_address) = (uint64_t)forwarded_argc;
            *(uint64_t *)(slide + (intptr_t)argv_address) = (uint64_t)(uintptr_t)forwarded_argv;
            return;
        }
        offset += command->cmdsize;
    }
}

static void prepare(void) {
    char executable[PATH_CAP];
    char bundle[PATH_CAP];
    char plan_path[PATH_CAP];
    char resources[PATH_CAP];
    char macos[PATH_CAP];
    char data_dir[PATH_CAP];
    char config_path[PATH_CAP];
    char log_path[PATH_CAP];
    char core_path[PATH_CAP];
    char content_path[PATH_CAP];
    char assets[PATH_CAP];
    char controls_defaults[PATH_CAP];
    char controls_override[PATH_CAP];
    char volume_path[PATH_CAP];
    char shader_choice[PATH_CAP];
    char shader_preset[PATH_CAP];
    char identity[128];
    char content[PATH_CAP];
    char title[PATH_CAP];
    char start_at_menu[8];
    char advanced[8];
    char volume_file[128];
    char shader_initial[PATH_CAP];
    char data_template[PATH_CAP];
    char managed[MANAGED_CAP][128];
    char *plan;
    const char *config_text;
    const char *home;
    size_t managed_count = 0;
    size_t index;
    uint32_t exec_path_size = sizeof executable;
    ConfigLine *lines = NULL;
    size_t line_count = 0;
    size_t line_capacity = 0;
    int log_fd;

    if (_NSGetExecutablePath(executable, &exec_path_size) != 0)
        die("could not find the launcher");
    if (!realpath(executable, bundle))
        die_errno("could not resolve the launcher");
    {
        char *slash = strrchr(bundle, '/');
        if (!slash)
            die("the launcher is not inside an app");
        *slash = '\0';
        slash = strrchr(bundle, '/');
        if (!slash)
            die("the launcher is not inside an app");
        *slash = '\0';
        slash = strrchr(bundle, '/');
        if (!slash)
            die("the launcher is not inside an app");
        *slash = '\0';
    }
    join_path(resources, sizeof resources, bundle, "Contents/Resources");
    join_path(macos, sizeof macos, bundle, "Contents/MacOS");
    join_path(plan_path, sizeof plan_path, resources, "launch.plan");
    plan = read_file(plan_path, NULL);
    if (!plan)
        die("the game is missing its launch plan");
    config_text = strstr(plan, "\n---config---\n");
    if (!config_text)
        die("the launch plan has no config");
    config_text += strlen("\n---config---\n");

    if (!field(plan, "identity", identity, sizeof identity) || strchr(identity, '/') || identity[0] == '\0')
        die("the launch plan has no identity");
    if (!field(plan, "content", content, sizeof content) || path_has_dotdot(content))
        die("the launch plan has no content path");
    if (!field(plan, "title", title, sizeof title))
        title[0] = '\0';
    field(plan, "start_at_menu", start_at_menu, sizeof start_at_menu);
    field(plan, "advanced", advanced, sizeof advanced);
    if (!field(plan, "volume_file", volume_file, sizeof volume_file))
        die("the launch plan has no volume file");
    field(plan, "shader_initial", shader_initial, sizeof shader_initial);
    if (!field(plan, "data_dir", data_template, sizeof data_template))
        die("the launch plan has no data directory");
    collect_managed(plan, managed, &managed_count);

    home = getenv("HOME");
    if (!home || home[0] != '/')
        die("HOME is not an absolute path, so there is nowhere safe to keep this game's files");
    if (starts_with(data_template, "$HOME")) {
        int wrote = snprintf(data_dir, sizeof data_dir, "%s%s", home, data_template + strlen("$HOME"));
        if (wrote < 0 || (size_t)wrote >= sizeof data_dir)
            die("the data directory does not fit");
    } else if (data_template[0] == '/') {
        snprintf(data_dir, sizeof data_dir, "%s", data_template);
    } else {
        die("the data directory is not absolute");
    }
    if (data_dir[0] != '/')
        die("the data directory is not absolute");
    /* In a sandbox, HOME is the container, so this is the container path.
     * Without the sandbox, HOME is the account's home, and this line would
     * create the player's real game directory. Refuse that instead. */
    if (writes_the_account_support_directory(data_dir))
        die("refusing to write the account's ROM-in-a-Box directory");

    migrate_previous_saves(data_dir);
    mkdir_p(data_dir);
    for (index = 0; index < managed_count; index++) {
        char directory[PATH_CAP];
        join_path(directory, sizeof directory, data_dir, managed[index]);
        mkdir_p(directory);
    }

    {
        char from[PATH_CAP];
        char to[PATH_CAP];
        join_path(from, sizeof from, resources, "remaps");
        join_path(to, sizeof to, data_dir, "remaps");
        seed_nested(from, to);
        join_path(from, sizeof from, resources, "autoconfig");
        join_path(to, sizeof to, data_dir, "autoconfig");
        seed_nested(from, to);
        join_path(from, sizeof from, resources, "core-options");
        join_path(to, sizeof to, data_dir, "config");
        seed_nested(from, to);
        join_path(from, sizeof from, resources, "firmware");
        join_path(to, sizeof to, data_dir, "system");
        seed_files(from, to);
    }

    load_base(&lines, &line_count, &line_capacity, config_text, data_dir, bundle);
    join_path(controls_defaults, sizeof controls_defaults, resources, "menu-assets/controls-defaults.cfg");
    join_path(controls_override, sizeof controls_override, data_dir, "controls.cfg");
    join_path(volume_path, sizeof volume_path, data_dir, volume_file);
    apply_player_file(&lines, &line_count, &line_capacity, controls_defaults);
    apply_player_file(&lines, &line_count, &line_capacity, controls_override);
    apply_player_file(&lines, &line_count, &line_capacity, volume_path);
    /* We take a screenshot with the window unfocused, where the console would
     * pause and the picture would show a stopped game. The author's
     * pause_nonactive is frozen, so we replace it here for this run instead of
     * writing the player's controls.cfg. */
    {
        const char *shot = getenv("ROMINABOX_MENU_SHOT");
        if (shot && shot[0])
            force_line(
                &lines,
                &line_count,
                &line_capacity,
                "pause_nonactive",
                "pause_nonactive = \"false\""
            );
    }

    shader_preset[0] = '\0';
    join_path(shader_choice, sizeof shader_choice, data_dir, "shader-choice");
    if (access(shader_choice, F_OK) == 0)
        first_line(shader_choice, shader_preset, sizeof shader_preset);
    else if (shader_initial[0]) {
        join_path(assets, sizeof assets, resources, "menu-assets");
        join_path(shader_preset, sizeof shader_preset, assets, shader_initial);
    }
    if (shader_preset[0])
        append_setting(&lines, &line_count, &line_capacity, "video_shader_enable = \"true\"");

    join_path(config_path, sizeof config_path, data_dir, "retroarch.cfg");
    write_config(config_path, lines, line_count);

    setenv("ROMINABOX_DATA_DIR", data_dir, 1);
    setenv("ROMINABOX_TITLE", title, 1);
    join_path(assets, sizeof assets, resources, "menu-assets");
    setenv("ROMINABOX_RML_ASSETS", assets, 1);
    setenv("ROMINABOX_ADVANCED_ACCESS", strcmp(advanced, "1") == 0 ? "1" : "0", 1);
    if (strcmp(start_at_menu, "1") == 0)
        setenv("ROMINABOX_START_AT_MENU", "1", 1);
    else
        unsetenv("ROMINABOX_START_AT_MENU");
    unsetenv("LIBRETRO_SYSTEM_DIRECTORY");
    unsetenv("LIBRETRO_DIRECTORY");
    unsetenv("LIBRETRO_ASSETS_DIRECTORY");
    unsetenv("LIBRETRO_AUTOCONFIG_DIRECTORY");
    unsetenv("LIBRETRO_CHEATS_DIRECTORY");
    unsetenv("LIBRETRO_DATABASE_DIRECTORY");
    unsetenv("LIBRETRO_VIDEO_FILTER_DIRECTORY");
    unsetenv("LIBRETRO_VIDEO_SHADER_DIRECTORY");

    join_path(log_path, sizeof log_path, data_dir, "logs/launch.log");
    log_fd = open(log_path, O_WRONLY | O_CREAT | O_APPEND, 0644);
    if (log_fd >= 0) {
        dup2(log_fd, STDOUT_FILENO);
        dup2(log_fd, STDERR_FILENO);
        if (log_fd > STDERR_FILENO)
            close(log_fd);
    }
    if (chdir(data_dir) != 0)
        die_errno(data_dir);

    join_path(core_path, sizeof core_path, resources, "game-core.dylib");
    join_path(content_path, sizeof content_path, resources, content);
    {
        const char *frames = getenv("ROMINABOX_MAX_FRAMES");
        const char *verbose = getenv("ROMINABOX_VERBOSE");
        static char frames_argument[32];
        int count = 0;
        forwarded_argv[count++] = strdup(executable);
        forwarded_argv[count++] = strdup("--config");
        forwarded_argv[count++] = strdup(config_path);
        forwarded_argv[count++] = strdup("--libretro");
        forwarded_argv[count++] = strdup(core_path);
        forwarded_argv[count++] = strdup(content_path);
        if (shader_preset[0]) {
            forwarded_argv[count++] = strdup("--set-shader");
            forwarded_argv[count++] = strdup(shader_preset);
        }
        if (verbose && strcmp(verbose, "1") == 0)
            forwarded_argv[count++] = strdup("--verbose");
        if (frames && frames[0]) {
            const char *digit = frames;
            while (*digit >= '0' && *digit <= '9')
                digit++;
            if (*digit != '\0' || strlen(frames) > 6)
                die("ROMINABOX_MAX_FRAMES is not a frame count");
            snprintf(frames_argument, sizeof frames_argument, "--max-frames=%s", frames);
            forwarded_argv[count++] = frames_argument;
        }
        if (count >= 12)
            die("too many launch arguments");
        forwarded_argv[count] = NULL;
        for (index = 0; index < (size_t)count; index++) {
            if (!forwarded_argv[index])
                die("out of memory");
        }
        forwarded_argc = count;
        publish_arguments();
    }
}

__attribute__((constructor)) static void start_launch(void) {
    prepare();
}
