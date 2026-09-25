#ifndef ROMINABOX_LAUNCHER_SHIPPED_FILES_H
#define ROMINABOX_LAUNCHER_SHIPPED_FILES_H

#include <stddef.h>

/* Files that we ship in an export and the player never chooses: firmware. On
 * every launch we replace the game's copy in `game` with each file in
 * `shipped` when the two differ, and remove a file that an earlier export
 * shipped and this one does not. `record`, in the game's data, lists what we
 * last shipped, one name per line. We touch nothing else in `game`.
 *
 * Returns 0, or -1 with errno set and the path that failed in `failed`. */
int rominabox_replace_shipped_files(
    const char *shipped,
    const char *game,
    const char *record,
    char *failed,
    size_t failed_cap);

#endif
