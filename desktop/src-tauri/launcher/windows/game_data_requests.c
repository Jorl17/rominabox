#include "game_data_requests.h"

#include <commdlg.h>
#include <stdio.h>
#include <string.h>

#include "game_data.h"
#include "../portable_fs.h"

/* The zip the player chose to import, until they confirm or choose again. */
static char chosen[RIB_GAME_DATA_PATH_SIZE];

/* Ask the player for a zip to save or open, in a dialog in front of the
 * game's window, which lets us take the foreground while it waits for us.
 * Returns 1 with the path in UTF-8 in `out`, or 0 when they closed it. */
static int ask_for_zip(HWND owner, int saving, const char *suggested, char *out, size_t out_size) {
    wchar_t file[RIB_GAME_DATA_PATH_SIZE] = L"";
    OPENFILENAMEW dialog;
    memset(&dialog, 0, sizeof dialog);
    if (suggested)
        MultiByteToWideChar(CP_UTF8, 0, suggested, -1, file, (int)(sizeof file / sizeof file[0]));
    dialog.lStructSize = sizeof dialog;
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = L"Zip files (*.zip)\0*.zip\0";
    dialog.lpstrFile = file;
    dialog.nMaxFile = (DWORD)(sizeof file / sizeof file[0]);
    dialog.lpstrDefExt = L"zip";
    dialog.lpstrTitle = saving ? L"Export the game's data" : L"Import data into the game";
    dialog.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR
        | (saving ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST);
    if (owner)
        SetForegroundWindow(owner);
    if (!(saving ? GetSaveFileNameW(&dialog) : GetOpenFileNameW(&dialog)))
        return 0;
    return WideCharToMultiByte(CP_UTF8, 0, file, -1, out, (int)out_size, NULL, NULL) > 0;
}

void game_data_request(int what, const char *data_dir, HWND owner, rib_pad_relay_data *reply) {
    char path[RIB_GAME_DATA_PATH_SIZE];
    char sentence[RIB_GAME_DATA_ERROR_SIZE] = "";
    rib_game_t *game = rib_games_new(1);
    rib_data_answer answer = RIB_DATA_FAILED;
    reply->title[0] = '\0';
    if (!game) {
        snprintf(reply->sentence, sizeof reply->sentence, "There is not enough memory.");
        reply->answer = RIB_DATA_FAILED;
        return;
    }
    switch (what) {
    case RIB_PAD_RELAY_EXPORT_DATA: {
        char name[RIB_GAME_DATA_TEXT_SIZE] = "Game data.zip";
        const char *folders[1];
        folders[0] = data_dir;
        if (rib_game_manifest_read(data_dir, game) == 0)
            rib_game_data_file_name(game, name, sizeof name);
        if (!ask_for_zip(owner, 1, name, path, sizeof path))
            answer = RIB_DATA_CANCELLED;
        else
            answer = rib_game_data_export(folders, 1, path, sentence, sizeof sentence) == 0 ? RIB_DATA_DONE
                                                                                             : RIB_DATA_FAILED;
        break;
    }
    case RIB_PAD_RELAY_CHOOSE_IMPORT:
        if (!ask_for_zip(owner, 0, NULL, chosen, sizeof chosen)) {
            chosen[0] = '\0';
            answer = RIB_DATA_CANCELLED;
            break;
        }
        switch (rib_game_data_choose(chosen, data_dir, game, sentence, sizeof sentence)) {
        case RIB_GAME_DATA_SAME_GAME:
            answer = RIB_DATA_DONE;
            break;
        case RIB_GAME_DATA_OTHER_GAME:
            answer = RIB_DATA_OTHER_GAME;
            break;
        default:
            chosen[0] = '\0';
            answer = RIB_DATA_FAILED;
            break;
        }
        snprintf(reply->title, sizeof reply->title, "%s", rib_game_get(game, "title"));
        break;
    case RIB_PAD_RELAY_CONFIRM_IMPORT: {
        char marker[RIB_GAME_DATA_PATH_SIZE];
        if (!chosen[0])
            snprintf(sentence, sizeof sentence, "Choose a zip to import first.");
        else if (rib_game_data_set_aside(chosen, data_dir, sentence, sizeof sentence) == 0) {
            /* We start the game again once it closes, and its launcher
             * imports the zip. */
            if (fs_join(marker, sizeof marker, data_dir, RIB_DATA_RESTART_MARKER) == 0
                && fs_write_file(marker, "", 0) == 0)
                answer = RIB_DATA_DONE;
            else
                snprintf(sentence, sizeof sentence, "We could not ask for the game to start again.");
            chosen[0] = '\0';
        }
        break;
    }
    default:
        snprintf(sentence, sizeof sentence, "The game asked for something we do not do.");
        break;
    }
    rib_games_free(game);
    snprintf(reply->sentence, sizeof reply->sentence, "%s", sentence);
    reply->answer = answer;
}
