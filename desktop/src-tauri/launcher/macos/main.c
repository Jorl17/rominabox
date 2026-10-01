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

#include <CoreFoundation/CoreFoundation.h>

#include "../launch.h"
#include "../portable_fs.h"
#include "../../../../vendor/retroarch/rominabox_launch.h"
#include "arguments.h"

/* launchd, the parent of what a person opens with a double-click or from
 * the Dock. */
#define LAUNCHD 1
/* The Application Support folder of a home, as a format string with the home. */
#define APPLICATION_SUPPORT_IN "%s/Library/Application Support"
/* Where an app keeps its own files, inside its bundle. */
#define BUNDLE_RESOURCES "Contents/Resources"
/* How many folders we keep open at once when we remove a game's data. */
#define FORGET_OPEN_FOLDERS 16
/* Anyone may read the launch log, and only its owner may write it. */
#define LOG_MODE 0644

#define RIB_CORE_FILE(platform, file) static const char core_##platform[] = file;
#include "../launch_contract.inc"

/* stdout is fully buffered when it is not a terminal. In the launcher we
 * point it at launch.log, so when the player is killed, or still running when
 * someone reads the log, RetroArch's lines stay in that buffer. */
void rominabox_line_buffer_stdio(void)
{
    setvbuf(stdout, NULL, _IOLBF, 0);
    setvbuf(stderr, NULL, _IOLBF, 0);
}

static void die_errno(const char *message) {
    char said[512];
    snprintf(said, sizeof said, "%s: %s", message, strerror(errno));
    rominabox_launch_die(said);
}

/* Whether a person opened this game, once we have found out. Until then we
 * do not show a failure. */
static int person_opened;

/* We run a Mac game's launcher inside the player before there is a window or
 * a run loop, so we show why a game cannot start in a CoreFoundation alert,
 * and wait for the person who opened it. We never show an alert in a quiet
 * run or a dry run. */
void rominabox_launch_tell(const char *message) {
    CFStringRef text;
    if (!rominabox_launch_tells_person(person_opened))
        return;
    text = CFStringCreateWithCString(kCFAllocatorDefault, message, kCFStringEncodingUTF8);
    if (!text)
        return;
    CFUserNotificationDisplayAlert(0, kCFUserNotificationStopAlertLevel, NULL, NULL, NULL, CFSTR(ROMINABOX_NAME),
                                   text, NULL, NULL, NULL, NULL);
    CFRelease(text);
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
        nftw(forget_folder, remove_entry, FORGET_OPEN_FOLDERS, FTW_DEPTH | FTW_PHYS);
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
    const char *test_user_data = getenv(RIB_ENV_TEST_USER_DATA);
    uint32_t exec_path_size = sizeof executable;
    struct passwd *user = getpwuid(getuid());
    LaunchPlaces places = {0};
    Launch launch;
    size_t index;
    int log_fd;

    /* When a person double-clicks the game or opens it from the Dock, it
     * starts through launchd. Otherwise a script or a harness started it. */
    person_opened = getppid() == LAUNCHD;
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
    rominabox_launch_join(resources, sizeof resources, bundle, BUNDLE_RESOURCES);

    places.resources = resources;
    places.core = core_Macos;
    places.user_data = user_data;
    if (test_user_data && test_user_data[0]) {
        /* A test's own folder replaces every per-user folder, for the game's
         * data and for QUICK SIGN IN, and there is no earlier data location. */
        if (test_user_data[0] != '/')
            rominabox_launch_die(RIB_ENV_TEST_USER_DATA " is not an absolute path");
        if (strlen(test_user_data) >= sizeof user_data)
            rominabox_launch_die("the data directory does not fit");
        strcpy(user_data, test_user_data);
        places.accounts_root = user_data;
    } else {
        /* A game's data is in its HOME's Application Support: inside the
         * sandbox, HOME is the game's container. */
        if (!home || home[0] != '/')
            rominabox_launch_die("HOME is not an absolute path, so there is nowhere safe to keep this game's files");
        {
            int wrote = snprintf(user_data, sizeof user_data, APPLICATION_SUPPORT_IN, home);
            if (wrote < 0 || (size_t)wrote >= sizeof user_data)
                rominabox_launch_die("the data directory does not fit");
        }
        /* QUICK SIGN IN's accounts are in the real home, which the sandbox's
         * HOME is not. */
        if (user && user->pw_dir && user->pw_dir[0] == '/') {
            int wrote = snprintf(accounts_root, sizeof accounts_root, APPLICATION_SUPPORT_IN, user->pw_dir);
            if (wrote > 0 && (size_t)wrote < sizeof accounts_root)
                places.accounts_root = accounts_root;
        }
        /* A game exported in the older layout kept its data in the real
         * home. Inside the sandbox, HOME is the container. */
        if (places.accounts_root) {
            char real_home[LAUNCH_PATH_CAP];
            char real_user[LAUNCH_PATH_CAP];
            if (!realpath(home, real_home) || !realpath(user->pw_dir, real_user) || strcmp(real_home, real_user) != 0)
                places.previous_user_data = accounts_root;
        }
    }
    places.opened_by_person = person_opened;
    rominabox_prepare_launch(&places, &launch);

    for (index = 0; index < launch.variable_count; index++) {
        if (launch.variables[index].value)
            setenv(launch.variables[index].name, launch.variables[index].value, 1);
        else
            unsetenv(launch.variables[index].name);
    }

    log_fd = open(launch.log_path, O_WRONLY | O_CREAT | O_APPEND, LOG_MODE);
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
    /* On RESET we remove the data folder only when it is directly inside the
     * games folder, as on Windows, because a game run without its sandbox,
     * such as a test build, has all of the person's rights. */
    {
        LaunchGame game;
        rominabox_read_game(resources, &game);
        if (rominabox_game_folder_to_forget(&game, user_data, forget_folder, sizeof forget_folder) == 0)
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
