#ifndef ROMINABOX_LAUNCHER_PLAYER_SETTINGS_H
#define ROMINABOX_LAUNCHER_PLAYER_SETTINGS_H

#include <stddef.h>

/* A setting that the player changes in the game's own menu, as declared in
 * the launch plan, one line each:
 *
 *   player_setting<TAB>file<TAB>key<TAB>default
 *
 * The player's choice is the `key = "value"` line that we write to `file` from
 * the menu, directly inside the game's data. Until there is one, we apply the
 * export's default. We never write the default to the player's file, so a
 * player who has not chosen gets the new default of a new export, and a
 * player who has chosen keeps that choice.
 *
 * `plan_line` is one line of the plan, without its line break. On success
 * `key` contains the RetroArch key and `line` the config line that applies,
 * `key = "value"`. Returns 1 for a setting, 0 for a plan line that is not one,
 * and -1 with errno set for a setting line that is malformed, lists a file
 * outside the data directory, or does not fit. */
int rominabox_player_setting(
    const char *data_dir,
    const char *plan_line,
    char *key,
    size_t key_cap,
    char *line,
    size_t line_cap);

#endif
