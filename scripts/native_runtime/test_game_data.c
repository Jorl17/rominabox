/* The zips of a game's data, as the player and the launcher use them: we play
 * a game's data into a folder, export it, set the zip aside as the menu does
 * for an import, and apply it as the launcher does before the next launch,
 * into a game whose saves are named after another file, all in the empty
 * folder named by its argument. The program exits by itself.
 * scripts/test_game_data.py compiles it with the same sources as the launcher
 * and removes the folder. The builder's tests reach the same code through
 * the engine. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "game_data.h"
#include "portable_fs.h"

static int failures;

static void expect(int condition, const char *what) {
   if (!condition) {
      fprintf(stderr, "FAIL %s\n", what);
      failures++;
   } else {
      printf("  ok   %s\n", what);
   }
}

static void put(const char *folder, const char *relative, const char *text) {
   char path[1024];
   char parent[1024];
   char *slash;
   snprintf(path, sizeof path, "%s/%s", folder, relative);
   snprintf(parent, sizeof parent, "%s", path);
   for (slash = parent + strlen(folder) + 1; (slash = strchr(slash, '/')); slash++) {
      *slash = '\0';
      fs_make_directory(parent);
      *slash = '/';
   }
   fs_write_file(path, text, strlen(text));
}

static int exists(const char *folder, const char *relative) {
   char path[1024];
   snprintf(path, sizeof path, "%s/%s", folder, relative);
   return fs_is_file(path);
}

static void manifest(const char *folder, const char *identity, const char *content) {
   rib_game_t *game = rib_games_new(1);
   fs_make_directory(folder);
   rib_game_set(game, "identity", identity);
   rib_game_set(game, "title", "Sonic 3");
   rib_game_set(game, "system", "megadrive");
   rib_game_set(game, "console", "Mega Drive / Genesis");
   rib_game_set(game, "content", content);
   rib_game_set(game, "player_file", "volume.cfg");
   rib_game_manifest_write(folder, game);
   rib_games_free(game);
}

int main(int argc, char **argv) {
   const char *root = argc > 1 ? argv[1] : NULL;
   char source[1024];
   char target[1024];
   char zip[1024];
   char error[RIB_GAME_DATA_ERROR_SIZE];
   rib_game_t *games;
   const char *exported[1];
   if (!root || !fs_is_directory(root))
      return 2;
   snprintf(source, sizeof source, "%s/source", root);
   snprintf(target, sizeof target, "%s/target", root);
   snprintf(zip, sizeof zip, "%s/Sonic 3.zip", root);

   manifest(source, "aaaaaaaaaaaaaaaaaaaaaaaa", "Sonic");
   put(source, "saves/Sonic.srm", "save");
   put(source, "states/Sonic.state1", "state");
   put(source, "volume.cfg", "audio_volume = \"-3.0\"\n");
   put(source, "achievements.session", "user\ntoken\n");
   exported[0] = source;
   expect(rib_game_data_export(exported, 1, zip, error, sizeof error) == 0, "we export a game's data");

   games = rib_games_new(4);
   expect(rib_game_data_list(zip, games, 4, error, sizeof error) == 1, "the zip contains one game");
   expect(!strcmp(rib_game_get(rib_games_at(games, 0), "title"), "Sonic 3"), "with its manifest");
   rib_games_free(games);

   manifest(target, "bbbbbbbbbbbbbbbbbbbbbbbb", "Sonic Patched");
   put(target, "states/Sonic Patched.state9", "newer state");
   expect(rib_game_data_check(zip, 0, target, NULL, error, sizeof error) == RIB_GAME_DATA_OTHER_GAME,
         "another game's data is for the same console");
   games = rib_games_new(1);
   expect(rib_game_data_choose(zip, target, games, error, sizeof error) == RIB_GAME_DATA_OTHER_GAME
         && !strcmp(rib_game_get(games, "identity"), "aaaaaaaaaaaaaaaaaaaaaaaa"),
         "the menu chooses the zip's only game");
   {
      char name[256];
      rib_game_set(games, "title", "Sonic: 3/Knuckles");
      rib_game_data_file_name(games, name, sizeof name);
      expect(!strcmp(name, "Sonic- 3-Knuckles data.zip"), "we suggest a file name every system allows");
   }
   {
      /* "A" and 22 katakana of three bytes each, longer than a name we
       * suggest, which ends at 63 bytes, inside the 21st katakana. */
      char title[128] = "A";
      char shortened[128] = "A";
      char name[256];
      int letter;
      for (letter = 0; letter < 22; letter++) {
         strcat(title, "\xe3\x82\xbd");
         if (letter < 20)
            strcat(shortened, "\xe3\x82\xbd");
      }
      strcat(shortened, " data.zip");
      rib_game_set(games, "title", title);
      rib_game_data_file_name(games, name, sizeof name);
      expect(!strcmp(name, shortened), "we shorten a long title between characters");
   }
   rib_games_free(games);
   expect(rib_game_data_set_aside(zip, target, error, sizeof error) == 0, "the menu sets the zip aside");
   expect(exists(target, "import.zip"), "beside the game's data");
   expect(rib_game_data_apply_pending(target, error, sizeof error) == 1, "the launcher imports it");
   expect(!exists(target, "import.zip"), "and removes it");
   expect(exists(target, "saves/Sonic Patched.srm") && exists(target, "states/Sonic Patched.state1"),
         "named after this game's file");
   expect(!exists(target, "states/Sonic Patched.state9"), "in place of the game's own states");
   expect(exists(target, "volume.cfg") && !exists(target, "achievements.session"), "with the settings, without the login");
   expect(rib_game_data_apply_pending(target, error, sizeof error) == 0, "with nothing set aside, nothing happens");

   /* A reset removes what an import replaces, and keeps the manifest, so
    * the game starts as new. */
   {
      char error_reset[RIB_GAME_DATA_ERROR_SIZE] = "";
      expect(rib_game_data_reset(target, error_reset, sizeof error_reset) == 0, "we reset a game's data");
      expect(!exists(target, "saves/Sonic Patched.srm") && !exists(target, "states/Sonic Patched.state1"),
             "its saves and states are gone");
      expect(!exists(target, "volume.cfg"), "and its settings");
      expect(exists(target, "game.json"), "and its manifest stays");
   }

   /* A game's data folder deep enough that the path of a save, with the
    * name we write it under first, is longer than Windows' 260 characters,
    * as for a long user name and a long game file name. */
   {
      char deep[1024];
      char error_deep[RIB_GAME_DATA_ERROR_SIZE] = "";
      snprintf(deep, sizeof deep, "%s/%0120d", root, 0);
      fs_make_directory(deep);
      snprintf(deep + strlen(deep), sizeof deep - strlen(deep), "/%0100d", 0);
      manifest(deep, "cccccccccccccccccccccccc", "Sonic Patched");
      expect(strlen(deep) + strlen("/saves/Sonic Patched.srm") > 260, "the deep folder's save path is over 260");
      expect(rib_game_data_import(zip, 0, deep, error_deep, sizeof error_deep) == 0,
             "we import into a game whose save path is longer than 260 characters");
      if (error_deep[0])
         fprintf(stderr, "     %s\n", error_deep);
      expect(exists(deep, "saves/Sonic Patched.srm"), "and its save is there");
   }
   return failures ? 1 : 0;
}
