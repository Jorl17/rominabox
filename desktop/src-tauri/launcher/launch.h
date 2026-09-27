#ifndef ROMINABOX_LAUNCHER_LAUNCH_H
#define ROMINABOX_LAUNCHER_LAUNCH_H

#include <stddef.h>

/* The work we do in a game's launcher before RetroArch starts, the same on
 * every platform. We read the launch plan from the export, create and fill the
 * game's data folder, write RetroArch's config, and set the variables for the
 * player and the arguments for RetroArch. In the entry for each platform
 * (macos/, windows/) we find its own files, apply the result and start the
 * player in that platform's way. */

#define LAUNCH_PATH_CAP 4096
#define LAUNCH_VARIABLES_CAP 32
#define LAUNCH_ARGUMENTS_CAP 12

/* Platform locations, and facts that only the platform entry can provide. */
typedef struct {
    /* The app's own files: the plan, the core, the menu. */
    const char *resources;
    /* The per-user application data folder the plan's $user_data stands
     * for, absolute. */
    const char *user_data;
    /* The per-user application data folder with the accounts for QUICK SIGN
     * IN, outside any sandbox, or NULL when there is none. */
    const char *accounts_root;
    /* The person started this game (not a test, a script or a harness). */
    int opened_by_person;
    /* Called with the data folder before we create it, or NULL for none. */
    void (*before_data_folder)(const char *data_dir);
} LaunchPlaces;

/* A variable the player reads. A NULL value clears it. */
typedef struct {
    const char *name;
    const char *value;
} LaunchVariable;

typedef struct {
    char data_dir[LAUNCH_PATH_CAP];
    char config_path[LAUNCH_PATH_CAP];
    char log_path[LAUNCH_PATH_CAP];
    int quiet;
    LaunchVariable variables[LAUNCH_VARIABLES_CAP];
    size_t variable_count;
    /* RetroArch's arguments, after its own name. */
    char *arguments[LAUNCH_ARGUMENTS_CAP];
    int argument_count;
} Launch;

/* Do everything before RetroArch starts. Stop the process with a message on
 * any failure, because a game without its plan or its data folder must not
 * start at all. */
void rominabox_prepare_launch(const LaunchPlaces *places, Launch *launch);

/* Quiet unless a person started the game or sound is turned on in the
 * environment. ROMINABOX_QUIET makes even that launch quiet. */
int rominabox_launch_is_quiet(int opened_by_person, const char *quiet, const char *sound);

void rominabox_launch_die(const char *message);

/* `left`, a separator unless it ends in one, and `right`. Stop the process
 * when the result does not fit. */
void rominabox_launch_join(char *out, size_t out_cap, const char *left, const char *right);

/* Makes `path` and every folder above it that is missing. */
void rominabox_launch_make_directories(const char *path);

#endif
