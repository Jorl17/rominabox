#ifndef ROMINABOX_LAUNCHER_SHIPPED_SETTINGS_H
#define ROMINABOX_LAUNCHER_SHIPPED_SETTINGS_H

#include <stddef.h>

/* Settings that we ship in an export and bring into the game's own data on
 * every launch, so a file left there by an earlier export or an older data
 * location never overrides the running app. We use this for core options,
 * remaps and controller profiles, which are RetroArch's key = "value" files,
 * laid out as <core or driver>/<file> under each root:
 *
 *   shipped  what this export sets, inside the app
 *   game     what RetroArch reads and rewrites, in the game's data
 *   applied  what came from an export at the last launch, in the game's data
 *
 * We replace the game's value with each shipped value unless the player
 * changed it after we last applied it. We remove a value that an earlier
 * export applied and this one no longer sets, so the default returns, and
 * every value of a file this export no longer ships. A game without its own
 * file gets the shipped one as it is. Other lines in the game's file stay.
 *
 * Returns 0, or -1 with errno set and the path that failed in `failed`. */
int rominabox_apply_shipped_settings(
    const char *shipped,
    const char *game,
    const char *applied,
    char *failed,
    size_t failed_cap);

#endif
