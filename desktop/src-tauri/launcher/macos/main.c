/* The macOS entry of a game's launcher, inside the player's own process. In
 * the export we inject this library into RetroArch. In its constructor we
 * prepare the launch (launch.c) before RetroArch's main starts, then pass
 * main its arguments through the trampoline in the frozen executable. */
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <ftw.h>
#include <limits.h>
#include <mach-o/dyld.h>
#include <pwd.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "../launch.h"
#include "../portable_fs.h"
#include "../../../../vendor/retroarch/rominabox_launch.h"
#include "arguments.h"

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

/* We run a Mac game's launcher inside the player before there is a window,
 * so we write the reason a game cannot start to its error stream. */
void rominabox_launch_tell(const char *message) {
    (void)message;
}

/* When the player chooses RESET in the menu, we leave RIB_FORGET_MARKER in
 * the game's data folder. When the player process ends, we remove the folder
 * with everything in it, including saves and settings, and keep the app. */
static char forget_folder[LAUNCH_PATH_CAP];

static int remove_entry(const char *path, const struct stat *facts, int kind, struct FTW *where) {
    (void)facts;
    (void)kind;
    (void)where;
    remove(path);
    return 0;
}

static void forget_if_asked(void) {
    char marker[LAUNCH_PATH_CAP];
    if (!forget_folder[0])
        return;
    rominabox_launch_join(marker, sizeof marker, forget_folder, RIB_FORGET_MARKER);
    if (fs_exists(marker))
        /* Deepest first. We remove a link itself and never follow it. */
        nftw(forget_folder, remove_entry, 16, FTW_DEPTH | FTW_PHYS);
}

static char *forwarded_argv[LAUNCH_ARGUMENTS_CAP + 1];
static int forwarded_argc;

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
    /* A game exported in the older layout kept its data in the real
     * home. Inside the sandbox, HOME is the container. */
    if (places.accounts_root) {
        char real_home[LAUNCH_PATH_CAP];
        char real_user[LAUNCH_PATH_CAP];
        if (!realpath(home, real_home) || !realpath(user->pw_dir, real_user) || strcmp(real_home, real_user) != 0)
            places.previous_user_data = accounts_root;
    }
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
    if (strlen(launch.data_dir) < sizeof forget_folder) {
        strcpy(forget_folder, launch.data_dir);
        atexit(forget_if_asked);
    }

    forwarded_argv[0] = strdup(executable);
    if (!forwarded_argv[0])
        rominabox_launch_die("out of memory");
    for (index = 0; index < (size_t)launch.argument_count; index++)
        forwarded_argv[index + 1] = launch.arguments[index];
    forwarded_argc = launch.argument_count + 1;
    forwarded_argv[forwarded_argc] = NULL;
    rominabox_publish_arguments(forwarded_argc, forwarded_argv);
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
