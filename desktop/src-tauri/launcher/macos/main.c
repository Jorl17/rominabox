/* The macOS entry of a game's launcher, inside the player's own process. In
 * the export we inject this library into RetroArch. In its constructor we
 * prepare the launch (launch.c) before RetroArch's main starts, then pass
 * main its arguments through the trampoline in the frozen executable. */
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

#include "../launch.h"

/* stdout is fully buffered when it is not a terminal. In the launcher we
 * point it at launch.log, so when the player is killed, or still running when
 * someone reads the log, RetroArch's lines stay in that buffer. */
void rominabox_line_buffer_stdio(void)
{
    setvbuf(stdout, NULL, _IOLBF, 0);
    setvbuf(stderr, NULL, _IOLBF, 0);
}

static void die_errno(const char *message) {
    fprintf(stderr, "ROM-in-a-Box: %s: %s\n", message, strerror(errno));
    exit(1);
}

static int starts_with(const char *value, const char *prefix) {
    return strncmp(value, prefix, strlen(prefix)) == 0;
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

static void copy_tree(const char *from_dir, const char *to_dir) {
    DIR *directory = opendir(from_dir);
    struct dirent *entry;
    if (!directory)
        return;
    rominabox_launch_make_directories(to_dir);
    while ((entry = readdir(directory))) {
        char from[LAUNCH_PATH_CAP];
        char to[LAUNCH_PATH_CAP];
        struct stat info;
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0)
            continue;
        rominabox_launch_join(from, sizeof from, from_dir, entry->d_name);
        if (lstat(from, &info) != 0)
            continue;
        if (S_ISLNK(info.st_mode))
            continue;
        rominabox_launch_join(to, sizeof to, to_dir, entry->d_name);
        if (S_ISDIR(info.st_mode))
            copy_tree(from, to);
        else if (S_ISREG(info.st_mode) && copy_file_if_absent(from, to) != 0)
            die_errno(from);
    }
    closedir(directory);
}

/* A game exported in the older layout kept its data in the real home.
 * Inside the sandbox, HOME is the container. On the first sandboxed launch we
 * copy what the old folder contains. */
static void migrate_previous_saves(const char *data_dir) {
    struct passwd *user = getpwuid(getuid());
    const char *home = getenv("HOME");
    char old_dir[LAUNCH_PATH_CAP];
    char marker[LAUNCH_PATH_CAP];
    char real_old[LAUNCH_PATH_CAP];
    char real_new[LAUNCH_PATH_CAP];
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
    rominabox_launch_join(marker, sizeof marker, data_dir, "retroarch.cfg");
    if (access(marker, F_OK) == 0 || access(old_dir, F_OK) != 0)
        return;
    copy_tree(old_dir, data_dir);
}

static char *forwarded_argv[LAUNCH_ARGUMENTS_CAP + 1];
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
    char executable[LAUNCH_PATH_CAP];
    char bundle[LAUNCH_PATH_CAP];
    char resources[LAUNCH_PATH_CAP];
    char accounts_root[LAUNCH_PATH_CAP];
    char user_data[LAUNCH_PATH_CAP];
    const char *home = getenv("HOME");
    uint32_t exec_path_size = sizeof executable;
    struct passwd *user = getpwuid(getuid());
    LaunchPlaces places = {0};
    Launch launch;
    size_t index;
    int log_fd;

    if (_NSGetExecutablePath(executable, &exec_path_size) != 0)
        rominabox_launch_die("could not find the launcher");
    if (!realpath(executable, bundle))
        die_errno("could not resolve the launcher");
    {
        char *slash = strrchr(bundle, '/');
        if (!slash)
            rominabox_launch_die("the launcher is not inside an app");
        *slash = '\0';
        slash = strrchr(bundle, '/');
        if (!slash)
            rominabox_launch_die("the launcher is not inside an app");
        *slash = '\0';
        slash = strrchr(bundle, '/');
        if (!slash)
            rominabox_launch_die("the launcher is not inside an app");
        *slash = '\0';
    }
    rominabox_launch_join(resources, sizeof resources, bundle, "Contents/Resources");

    places.resources = resources;
    /* A game's data is in its HOME's Application Support: inside the
     * sandbox, HOME is the game's container. */
    if (!home || home[0] != '/')
        rominabox_launch_die("HOME is not an absolute path, so there is nowhere safe to keep this game's files");
    {
        int wrote = snprintf(user_data, sizeof user_data, "%s/Library/Application Support", home);
        if (wrote < 0 || (size_t)wrote >= sizeof user_data)
            rominabox_launch_die("the data directory does not fit");
    }
    places.user_data = user_data;
    /* QUICK SIGN IN's accounts are in the real home, which the sandbox's
     * HOME is not. */
    if (user && user->pw_dir && user->pw_dir[0] == '/') {
        int wrote = snprintf(accounts_root, sizeof accounts_root, "%s/Library/Application Support", user->pw_dir);
        if (wrote > 0 && (size_t)wrote < sizeof accounts_root)
            places.accounts_root = accounts_root;
    }
    /* When a person double-clicks the game or opens it from the Dock, it
     * starts through launchd. Otherwise a script or a harness started it. */
    places.opened_by_person = getppid() == 1;
    places.before_data_folder = migrate_previous_saves;
    rominabox_prepare_launch(&places, &launch);

    for (index = 0; index < launch.variable_count; index++) {
        if (launch.variables[index].value)
            setenv(launch.variables[index].name, launch.variables[index].value, 1);
        else
            unsetenv(launch.variables[index].name);
    }

    log_fd = open(launch.log_path, O_WRONLY | O_CREAT | O_APPEND, 0644);
    if (log_fd >= 0) {
        dup2(log_fd, STDOUT_FILENO);
        dup2(log_fd, STDERR_FILENO);
        if (log_fd > STDERR_FILENO)
            close(log_fd);
        rominabox_line_buffer_stdio();
    }
    if (launch.quiet) {
        fprintf(stdout, "[RIB] quiet: audio driver null, output disabled\n");
        fflush(stdout);
    }
    if (chdir(launch.data_dir) != 0)
        die_errno(launch.data_dir);

    forwarded_argv[0] = strdup(executable);
    if (!forwarded_argv[0])
        rominabox_launch_die("out of memory");
    for (index = 0; index < (size_t)launch.argument_count; index++)
        forwarded_argv[index + 1] = launch.arguments[index];
    forwarded_argc = launch.argument_count + 1;
    forwarded_argv[forwarded_argc] = NULL;
    publish_arguments();
}

#ifdef ROMINABOX_PLAN_MAIN
int main(void) {
    prepare();
    return 0;
}
#else
__attribute__((constructor)) static void start_launch(void) {
    prepare();
}
#endif
