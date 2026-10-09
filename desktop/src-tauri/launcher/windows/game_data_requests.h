/* The game's data, which a sandboxed game asks us to export and import
 * through the controller relay (pad_relay.c), because only here, outside its
 * sandbox, can we show a file dialog and reach the file the player chooses.
 * The zips themselves are the shared code's (gamedata/game_data.h). */
#ifndef ROMINABOX_LAUNCHER_GAME_DATA_REQUESTS_H
#define ROMINABOX_LAUNCHER_GAME_DATA_REQUESTS_H

#define WIN32_LEAN_AND_MEAN
#define DIRECTINPUT_VERSION 0x0800
#include <windows.h>

#include "../../../../vendor/retroarch/rominabox_pad_relay.h"

/* Perform `what` (RIB_PAD_RELAY_EXPORT_DATA, CHOOSE_IMPORT or CONFIRM_IMPORT)
 * for the game whose data is in `data_dir`, with our dialogs owned by
 * `owner`, and write what happened in `reply`. When a dialog closes, we give
 * the foreground back to `game_window`, the game's window, when it is known.
 * After CONFIRM_IMPORT, we have set the chosen zip aside and left
 * RIB_DATA_RESTART_MARKER in `data_dir`. */
void game_data_request(int what, const char *data_dir, HWND owner, HWND game_window, rib_pad_relay_data *reply);

#endif
