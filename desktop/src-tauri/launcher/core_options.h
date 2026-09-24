#ifndef ROMINABOX_LAUNCHER_CORE_OPTIONS_H
#define ROMINABOX_LAUNCHER_CORE_OPTIONS_H

#include <stddef.h>

/* Bring the game's own core options files in line with the ones the export
 * ships. All three roots contain <core>/<file>.opt, the layout RetroArch reads:
 *
 *   shipped  what this export sets, inside the app
 *   game     what RetroArch reads and rewrites, in the game's data
 *   applied  what came from an export at the last launch, in the game's data
 *
 * We replace the game's value with each shipped value unless the player
 * changed it after we last applied it. We remove a value that an earlier
 * export applied and this one no longer sets, so the core's own default
 * returns. Any other line in the game's file stays as it is.
 *
 * Returns 0, or -1 with errno set and the path that failed in `failed`. */
int rominabox_apply_core_options(
    const char *shipped,
    const char *game,
    const char *applied,
    char *failed,
    size_t failed_cap);

#endif
