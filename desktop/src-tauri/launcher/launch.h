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
    /* Inside a sandbox, the per-user folder outside it, where a game exported
     * in the older layout kept its data. On the first sandboxed launch we copy
     * what it contains. NULL when the game is not in a sandbox. */
    const char *previous_user_data;
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

/* Which game this is and what it can reach outside its own data, from its
 * launch plan. On macOS we declare a game's sandbox when we sign it in the
 * export. On Windows we set up the sandbox in the launcher before the launch. */
typedef struct {
    char identity[128];
    char title[LAUNCH_PATH_CAP];
    /* The data folder, with $user_data standing for the per-user folder. */
    char data_template[LAUNCH_PATH_CAP];
    /* The network, and the QUICK SIGN IN folder when accounts_name is set. */
    int achievements;
    char accounts_name[128];
    /* The game runs in its own sandbox. */
    int sandbox;
} LaunchGame;

/* Read the plan in `resources`. Stop the process with a message when the
 * game has no plan or the plan does not contain the game's identity. */
void rominabox_read_game(const char *resources, LaunchGame *game);

/* The game's data folder when `user_data` is the per-user folder. */
void rominabox_game_data_folder(const LaunchGame *game, const char *user_data, char *out, size_t out_cap);

/* The game's data folder when `user_data` is the per-user folder, for
 * removal. 0 when it is directly inside the games folder (RIB_USER_FOLDER
 * Games, launch_contract.inc), and -1, with `out` untouched, for any other
 * folder, which we never remove. */
int rominabox_game_folder_to_forget(const LaunchGame *game, const char *user_data, char *out, size_t out_cap);

/* Do everything before RetroArch starts. Stop the process with a message on
 * any failure, because a game without its plan or its data folder must not
 * start at all. */
void rominabox_prepare_launch(const LaunchPlaces *places, Launch *launch);

/* RIB_ENV_QUIET is one switch for an automated run, and a person who opens
 * the game does not set it. Without it, a screenshot run would open an output
 * device and play sound. ROMINABOX_SOUND turns sound on in any case, and we
 * read it only in the launcher. */
#define ROMINABOX_SOUND_ENV "ROMINABOX_SOUND"

/* Quiet unless a person started the game or sound is turned on in the
 * environment. ROMINABOX_QUIET makes even that launch quiet. */
int rominabox_launch_is_quiet(int opened_by_person, const char *quiet, const char *sound);

void rominabox_launch_die(const char *message);

/* Tell a person who opened the game why it cannot start, where the platform
 * allows it. We define it in the launcher for each platform. In a quiet run,
 * or a run that stops after its plan, we only write to the error stream. */
void rominabox_launch_tell(const char *message);

/* `left`, a separator unless it ends in one, and `right`. Stop the process
 * when the result does not fit. */
void rominabox_launch_join(char *out, size_t out_cap, const char *left, const char *right);

/* Makes `path` and every folder above it that is missing. */
void rominabox_launch_make_directories(const char *path);

#endif
