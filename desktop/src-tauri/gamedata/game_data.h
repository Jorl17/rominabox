/* A game's data as one portable zip: the player's saves, states,
 * screenshots, controls and settings, which a person can export from the
 * game or the builder and import into the same game or another game for the
 * same console.
 *
 * Each game's data folder contains its manifest (`game.json`), which the
 * launcher writes on every launch: what the game is, what its saves are named
 * after, and which files hold the player's settings. A backup contains the
 * manifest and the files we declare as the player's own, and nothing else:
 *
 *   - the folders in RIB_PLAYER_FOLDER and RIB_SHIPPED_SETTINGS
 *     (launch_contract.inc), with RIB_GAME_DATA(Applied), whatever is in them
 *   - the files the menu writes, RIB_DATA_FILE (declarations.inc)
 *   - the player-setting files the manifest lists
 *
 * So a backup never contains the achievements login, the BIOS, the config we
 * write again on every launch, caches or logs. A zip may contain one game at
 * its top, or several, each in a folder of its own (a bulk backup).
 *
 * We read a zip as untrusted. We refuse it whole when an entry is not one of
 * the declared paths, a path climbs out of its folder, it is larger than we
 * allow, or its game is for another console. When we import a backup into a
 * game whose saves are named after another file, we rename them to match.
 *
 * This is ROM-in-a-Box's own code, on top of miniz and of libretro-common's
 * rjson for the manifest. The player, the launcher and the builder compile
 * this one file. */
#ifndef ROMINABOX_GAME_DATA_H
#define ROMINABOX_GAME_DATA_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* The manifest format we write, and the newest we read. */
#define RIB_GAME_DATA_FORMAT 1

#define RIB_GAME_DATA_TEXT_SIZE 256
#define RIB_GAME_DATA_PATH_SIZE 2048
#define RIB_GAME_DATA_ID_SIZE 64
#define RIB_GAME_DATA_PLAYER_FILES 16
/* The most games one zip may contain. */
#define RIB_GAME_DATA_GAMES 512
/* A sentence for a person about why something failed. */
#define RIB_GAME_DATA_ERROR_SIZE 1024

/* What a manifest says about a game. Text contains no control character. */
typedef struct rib_game {
   char identity[RIB_GAME_DATA_ID_SIZE];
   char title[RIB_GAME_DATA_TEXT_SIZE];
   /* The console's id, as in desktop/systems.json, and its name. */
   char system[RIB_GAME_DATA_ID_SIZE];
   char console[RIB_GAME_DATA_TEXT_SIZE];
   /* The name of the game's file without its extension: RetroArch names
    * saves, states and screenshots after it. */
   char content[RIB_GAME_DATA_TEXT_SIZE];
   /* Where the app was when the game last started, and the version of
    * ROM-in-a-Box that made it. Either may be empty. */
   char app[RIB_GAME_DATA_PATH_SIZE];
   char made_with[RIB_GAME_DATA_ID_SIZE];
   char player_files[RIB_GAME_DATA_PLAYER_FILES][RIB_GAME_DATA_ID_SIZE];
   size_t player_file_count;
} rib_game_t;

/* What checking a backup against a game found. */
typedef enum rib_game_data_check {
   /* A backup of this game. */
   RIB_GAME_DATA_SAME_GAME = 0,
   /* A backup of another game for the same console: we can import it, and
    * we tell the person which game it is from first. */
   RIB_GAME_DATA_OTHER_GAME = 1,
   /* We cannot import it, and `error` says why. */
   RIB_GAME_DATA_REFUSED = -1
} rib_game_data_check_t;

/* A field of `game` by its name in the manifest (identity, title, system,
 * console, content, app, made_with), or NULL for another name. */
const char *rib_game_get(const rib_game_t *game, const char *name);
/* Set a field by its name, or with `player_file`, add a player-setting file.
 * Returns 0, or -1 for another name or a full list. */
int rib_game_set(rib_game_t *game, const char *name, const char *value);
size_t rib_game_player_file_count(const rib_game_t *game);
const char *rib_game_player_file(const rib_game_t *game, size_t which);
/* Room for `count` empty games, the `which`th of them, and freeing them, for
 * a caller that does not know the layout of rib_game_t. */
rib_game_t *rib_games_new(size_t count);
rib_game_t *rib_games_at(rib_game_t *games, size_t which);
void rib_games_free(rib_game_t *games);

/* Write `game` as the manifest in `data_dir`. Returns 0, or -1 with errno
 * set. */
int rib_game_manifest_write(const char *data_dir, const rib_game_t *game);
/* Read the manifest in `data_dir` into `game`. Returns 0, or -1 when there
 * is none or we cannot read it. */
int rib_game_manifest_read(const char *data_dir, rib_game_t *game);

/* Write a zip of the data of the games in `data_dirs` to `zip_path`. With one
 * folder, the game is at the top of the zip; with more, each is in a folder
 * of its own, named after its title and identity. Each folder must contain a
 * manifest. Returns 0, or -1 with a sentence in `error`. */
int rib_game_data_export(const char *const *data_dirs, size_t count, const char *zip_path,
      char *error, size_t error_size);

/* The games in the zip at `zip_path`, in the order of the zip, into `games`,
 * which has room for `capacity` of them. Returns how many there are, or -1
 * with a sentence in `error` when the zip is not a backup we can read: we
 * check every entry here, so a zip we list is one we can import. */
int rib_game_data_list(const char *zip_path, rib_game_t *games, size_t capacity,
      char *error, size_t error_size);

/* Whether the `which`th game in the zip can go into the game whose data is in
 * `data_dir`, and the backup's game in `source` when it is not NULL. */
rib_game_data_check_t rib_game_data_check(const char *zip_path, size_t which, const char *data_dir,
      rib_game_t *source, char *error, size_t error_size);

/* For a zip a player chose in the menu of the game whose data is in
 * `data_dir`: in a bulk backup the game's own data, and else the zip's only
 * game, checked as in rib_game_data_check. */
rib_game_data_check_t rib_game_data_choose(const char *zip_path, const char *data_dir, rib_game_t *source,
      char *error, size_t error_size);

/* The name we suggest for an export of `game`'s data: its title, with
 * anything a file system may refuse in a name replaced, and " data.zip". */
void rib_game_data_file_name(const rib_game_t *game, char *out, size_t size);

/* Replace the player's files in `data_dir` with those of the `which`th game
 * in the zip, renaming saves, states and screenshots named after the
 * backup's file to the name this game's saves have. We check the whole zip
 * before we change anything. The manifest in `data_dir` stays. Returns 0, or
 * -1 with a sentence in `error`. */
int rib_game_data_import(const char *zip_path, size_t which, const char *data_dir,
      char *error, size_t error_size);

/* An import the player chose in the game, which the launcher applies before
 * the next launch: copy the zip into `data_dir` (RIB_GAME_FILE(PendingImport)).
 * Returns 0, or -1 with a sentence in `error`. */
int rib_game_data_set_aside(const char *zip_path, const char *data_dir, char *error, size_t error_size);
/* Import the zip set aside in `data_dir`, if there is one, and remove it.
 * Returns 1 when we imported one, 0 when there was none, and -1 with a
 * sentence in `error` when there was one we could not import, which we
 * remove too. */
int rib_game_data_apply_pending(const char *data_dir, char *error, size_t error_size);

#ifdef __cplusplus
}
#endif

#endif
