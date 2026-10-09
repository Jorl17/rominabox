#ifndef ROMINABOX_LAUNCHER_LAUNCH_GAME_DATA_H
#define ROMINABOX_LAUNCHER_LAUNCH_GAME_DATA_H

#include <stddef.h>

#include "game_data.h"

/* What a launch does in the game's data folder `data_dir` before anything
 * reads it. We write `manifest` there, so that it names where the app is
 * now, and copy the game's icon at `icon` beside it; when we cannot, we say
 * so on stderr and carry on. We remove a request to restart left by a run
 * that has ended, and import the backup the player chose in that run's menu.
 *
 * Returns 0, or -1 with the reason in `error` when we could not import the
 * backup. */
int rominabox_launch_game_data(
    const char *data_dir,
    const rib_game_t *manifest,
    const char *icon,
    char *error,
    size_t error_size);

#endif
