#include "launch_game_data.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>

#include "launch.h"
#include "portable_fs.h"
#include "../../../vendor/retroarch/rominabox_game_data.h"

#define RIB_GAME_FILE(name, path) static const char game_file_##name[] = path;
#include "launch_contract.inc"

int rominabox_launch_game_data(
    const char *data_dir,
    const rib_game_t *manifest,
    const char *icon,
    char *error,
    size_t error_size) {
    char path[RIB_GAME_DATA_PATH_SIZE];
    if (rib_game_manifest_write(data_dir, manifest) != 0)
        fprintf(stderr, ROMINABOX_NAME ": could not write the game's manifest: %s\n", strerror(errno));
    if (fs_is_file(icon) && fs_join(path, sizeof path, data_dir, game_file_Icon) == 0
            && (fs_remove(path) != 0 || fs_copy_new(icon, path) != 0))
        fprintf(stderr, ROMINABOX_NAME ": could not copy the game's icon: %s\n", strerror(errno));
    /* A request to restart is for the run that made it, which has ended. */
    if (fs_join(path, sizeof path, data_dir, RIB_DATA_RESTART_MARKER) == 0)
        fs_remove(path);
    return rib_game_data_apply_pending(data_dir, error, error_size) < 0 ? -1 : 0;
}
