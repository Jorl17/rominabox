/* Native managed-session integration: actual rc_client/runtime, synthetic
 * HTTP. No request leaves this process, and we use no account or award. */
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include "native_runtime/test_environment.h"
#include "native_runtime/test_folders.h"
#include "portable_fs.h"

#include "../vendor/retroarch/cheevos/rominabox.h"
#include "../vendor/retroarch/cheevos/rominabox_internal.h"
#include "../vendor/retroarch/cheevos/rominabox_catalog.h"
#include "../vendor/retroarch/cheevos/cheevos_locals.h"
#include "../vendor/retroarch/configuration.h"

static settings_t settings;
static rcheevos_locals_t locals;
static uint8_t memory_byte;
static unsigned awards;
static unsigned badge_downloads;
/* While the menu is open, every badge is fetched in the RetroArch queue,
 * and a second download of a file already being fetched is rejected. */
static bool badge_already_downloading;
static int64_t now_usec = 1000000;

int64_t cpu_features_get_time_usec(void)
{
   return now_usec;
}
static bool defer_login;
static bool fail_login;
static bool defer_award;
static bool reject_award;
static bool replace_account;
static bool expire_token;
static bool refuse_password;
static unsigned fail_award_attempts;
static rc_client_server_callback_t deferred_callback;
static void *deferred_data;
static rc_client_server_callback_t deferred_award_callback;
static void *deferred_award_data;
static unsigned deferred_award_generation;
static rc_clock_t fake_time;

static const char login_json[] =
   "{\"Success\":true,\"User\":\"Fixture\",\"Token\":\"fixture-token\"}";
static const char login_error_json[] =
   "{\"Success\":false,\"Error\":\"Synthetic network failure\"}";
static const char expired_login_json[] =
   "{\"Success\":false,\"Error\":\"The token has expired.\",\"Code\":\"expired_token\"}";
static const char refused_login_json[] =
   "{\"Success\":false,\"Error\":\"Wrong password.\",\"Code\":\"invalid_credentials\"}";
static const char replacement_login_json[] =
   "{\"Success\":true,\"User\":\"Other\",\"Token\":\"other-token\"}";
static const char game_json[] =
   "{\"Success\":true,\"GameId\":1,\"Title\":\"Fixture Game\","
   "\"ConsoleId\":1,\"ImageIconUrl\":\"/Images/1.png\","
   "\"RichPresencePatch\":\"\",\"Sets\":[{\"AchievementSetId\":1,"
   "\"GameId\":1,\"Title\":\"Fixture Game\",\"Type\":\"core\","
   "\"ImageIconUrl\":\"/Images/1.png\",\"Achievements\":[{"
   "\"ID\":123,\"Title\":\"First step\","
   "\"Description\":\"Reach one\",\"Flags\":3,\"Points\":5,"
   "\"MemAddr\":\"0xH0000=1.3.\",\"Author\":\"Fixture\","
   "\"BadgeName\":\"123\",\"Created\":1,\"Modified\":1}],"
   "\"Leaderboards\":[]}]}";
static const char session_json[] =
   "{\"Success\":true,\"Unlocks\":[],\"HardcoreUnlocks\":[]}";
static const char award_json[] =
   "{\"Success\":true,\"AchievementID\":123,\"Score\":0,"
   "\"SoftcoreScore\":5,\"AchievementsRemaining\":0}";
static const char reject_award_json[] =
   "{\"Success\":false,\"Error\":\"Synthetic award rejection\"}";

static rc_clock_t clock_millisecs(const rc_client_t *client)
{
   (void)client;
   return fake_time;
}

static uint32_t read_memory(uint32_t address, uint8_t *buffer,
      uint32_t bytes, rc_client_t *client)
{
   (void)client;
   if (address != 0 || bytes != 1)
      return 0;
   buffer[0] = memory_byte;
   return 1;
}

static void on_event(const rc_client_event_t *event, rc_client_t *client)
{
   (void)client;
   rib_achievements_event(event);
}

static void server(const rc_api_request_t *request,
      rc_client_server_callback_t callback, void *data, rc_client_t *client)
{
   rc_api_server_response_t response = {0};
   const char *body = "{\"Success\":false,\"Error\":\"Unexpected request\"}";
   const char *post = request->post_data ? request->post_data : "";
   (void)client;
   if (strstr(post, "r=login2"))
   {
      if (defer_login)
      {
         deferred_callback = callback;
         deferred_data = data;
         return;
      }
      body = fail_login ? login_error_json :
            (replace_account ? replacement_login_json : login_json);
      if (expire_token && strstr(post, "t="))
         body = expired_login_json;
      if (refuse_password && strstr(post, "p="))
         body = refused_login_json;
   }
   else if (strstr(post, "r=gameid"))
      body = "{\"Success\":true,\"GameID\":1}";
   else if (strstr(post, "r=achievementsets"))
      body = game_json;
   else if (strstr(post, "r=startsession"))
      body = session_json;
   else if (strstr(post, "r=awardachievement"))
   {
      unsigned generation = rib_achievements_award_request_started();
      ++awards;
      if (defer_award)
      {
         deferred_award_callback = callback;
         deferred_award_data = data;
         deferred_award_generation = generation;
         return;
      }
      if (fail_award_attempts)
      {
         --fail_award_attempts;
         response.http_status_code = 503;
      }
      body = response.http_status_code == 503 ? "" :
            (reject_award ? reject_award_json : award_json);
      response.body = body;
      response.body_length = strlen(body);
      if (response.http_status_code != 503)
         response.http_status_code = 200;
      callback(&response, data);
      rib_achievements_award_request_finished(generation);
      return;
   }
   else if (strstr(post, "r=ping"))
      body = "{\"Success\":true}";
   else
   {
      fprintf(stderr, "Unexpected synthetic request: %s\n", post);
      abort();
   }
   response.body = body;
   response.body_length = strlen(body);
   response.http_status_code = 200;
   callback(&response, data);
}

settings_t *config_get_ptr(void) { return &settings; }
rcheevos_locals_t *get_rcheevos_locals(void) { return &locals; }


bool rcheevos_client_download_badge_from_url(const char *url,
      const char *badge_name)
{
   (void)url;
   (void)badge_name;
   if (badge_already_downloading)
      return false;
   ++badge_downloads;
   return true;
}

rc_client_t *rcheevos_rib_prepare_client(void)
{
   if (locals.client)
      rc_client_unload_game(locals.client);
   else
   {
      locals.client = rc_client_create(read_memory, server);
      assert(locals.client);
      rc_client_set_event_handler(locals.client, on_event);
      rc_client_set_get_time_millisecs_function(locals.client,
            clock_millisecs);
   }
   return locals.client;
}

rc_client_async_handle_t *rcheevos_rib_begin_identify(
      const struct retro_game_info *info, rc_client_callback_t callback,
      void *userdata)
{
   (void)info;
   return rc_client_begin_load_game(locals.client,
         "0123456789abcdef0123456789abcdef", callback, userdata);
}

void rcheevos_rib_complete_game_load(int result, const char *error,
      rc_client_t *client, void *userdata)
{
   (void)error;
   (void)client;
   (void)userdata;
   assert(result == RC_OK);
   locals.core_supports = true;
}

static rib_achievements_snapshot_t snapshot(void)
{
   rib_achievements_snapshot_t value;
   rib_achievements_get_snapshot(&value);
   return value;
}

static void ready(void)
{
   unsigned i;
   for (i = 0; i < 8 && snapshot().status != RIB_ACHIEVEMENTS_ACTIVE; ++i)
      rib_achievements_pump();
   assert(snapshot().status == RIB_ACHIEVEMENTS_ACTIVE);
}

/* The badge of a row from request to picture. In the download callback we
 * report a failure by name. A refresh of the rows keeps the badge state.
 * When the list is shown again, we request again a badge that failed or got
 * no answer. Leaves 123_lock.png downloaded. */
static void badge_lifecycle(const char *directory)
{
   rib_achievement_row_t row;
   uint32_t revision;
   FILE *file;
   char badge_dir[512];
   char badge_path[512];

   assert(rib_achievements_get_row(0, &row));
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_LOADING && badge_downloads == 1);

   /* We repeat a request that got no answer when the list opens again. */
   rib_achievements_list_shown(true);
   rib_achievements_list_shown(false);
   rib_achievements_list_shown(true);
   assert(rib_achievements_get_row(0, &row));
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_LOADING);
   assert(badge_downloads == 2);

   /* The player sees a badge downloading or shown, never "not fetched". We
    * treat a failed download as still on its way, and request it again after
    * a pause, while the menu runs. */
   rib_achievements_badge_failed("123_lock");
   assert(rib_achievements_get_row(0, &row));
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_LOADING && badge_downloads == 2);
   rib_catalog_mark_rows_dirty();
   assert(rib_achievements_get_row(0, &row));
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_LOADING && badge_downloads == 2);
   snapshot();
   assert(badge_downloads == 2);
   now_usec += 10 * 1000000;
   snapshot();
   assert(rib_achievements_get_row(0, &row));
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_LOADING && badge_downloads == 3);

   /* The menu opens while the RetroArch queue is already fetching the badge,
    * so the request for the row is rejected. We show the badge as on its way
    * until the picture from the queue arrives. */
   badge_already_downloading = true;
   rib_achievements_list_shown(false);
   rib_achievements_list_shown(true);
   assert(rib_achievements_get_row(0, &row));
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_LOADING && badge_downloads == 3);
   badge_already_downloading = false;

   /* In the task callback we mark badges dirty. In the next main-thread
    * snapshot we publish a revision and copied path without another get_row. */
   revision = snapshot().revision;
   snprintf(badge_dir, sizeof(badge_dir), "%s/achievements-badges", directory);
   assert(fs_make_directory(badge_dir) == 0);
   snprintf(badge_path, sizeof(badge_path), "%s/123_lock.png", badge_dir);
   file = fopen(badge_path, "wb");
   assert(file);
   assert(fclose(file) == 0);
   rib_achievements_badge_downloaded();
   assert(snapshot().revision > revision);
   assert(rib_achievements_get_row(0, &row));
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_READY);
   assert(strstr(row.badge_path, "123_lock.png") != NULL);
   assert(badge_downloads == 3);
}

/* The colour badge requested at an unlock arrives, and we show it in its row.
 * We remove the file again, so after later refreshes the row has no picture. */
static void colour_badge_arrives(const char *directory)
{
   rib_achievement_row_t row;
   char badge_path[512];
   FILE *file;
   snprintf(badge_path, sizeof(badge_path), "%s/achievements-badges/123.png", directory);
   file = fopen(badge_path, "wb");
   assert(file);
   assert(fclose(file) == 0);
   rib_achievements_badge_downloaded();
   assert(rib_achievements_get_row(0, &row));
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_READY);
   assert(strstr(row.badge_path, "123.png") != NULL);
   assert(unlink(badge_path) == 0);
}

static void unload(void)
{
   rib_achievements_content_unload();
   rc_client_destroy(locals.client);
   locals.client = NULL;
}

/* Pumps until the login in progress has an answer. */
static void answered(void)
{
   unsigned i;
   for (i = 0; i < 8 && snapshot().status == RIB_ACHIEVEMENTS_SIGNING_IN; ++i)
      rib_achievements_pump();
   assert(snapshot().status != RIB_ACHIEVEMENTS_SIGNING_IN);
}

static size_t saved_accounts(rib_achievements_saved_account_t *saved)
{
   return rib_achievements_saved_accounts(saved, 4);
}

static void play(const char *directory, const char *game, const struct retro_game_info *info)
{
   test_setenv("ROMINABOX_DATA_DIR", directory);
   test_setenv("ROMINABOX_GAME_IDENTITY", game);
   assert(rib_achievements_content_load(info));
}

#define GAME_A "aaaaaaaaaaaaaaaaaaaaaaaa"
#define GAME_B "bbbbbbbbbbbbbbbbbbbbbbbb"

/* QUICK SIGN IN between two games that share one accounts folder, each with
 * separate storage. We test the rules of the store in the accounts tests.
 * Here we test when we save, use, drop and forget an account in the session. */
static void shared_accounts(const struct retro_game_info *info,
      const char *first, const char *second)
{
   char accounts[512];
   char path[512];
   char line[256];
   rib_achievements_saved_account_t saved[4];
   FILE *file;
   test_temporary_folder(accounts, sizeof accounts, "rib-achievements-accounts-");
   test_setenv("ROMINABOX_ACCOUNTS_DIR", accounts);
   replace_account = false;

   /* Game A: after a password sign-in we save the account for the others. */
   play(first, GAME_A, info);
   assert(saved_accounts(saved) == 0);
   assert(rib_achievements_sign_in("Fixture", "fixture-password"));
   ready();
   assert(saved_accounts(saved) == 1);
   assert(!strcmp(saved[0].username, "Fixture") && !strcmp(saved[0].display_name, "Fixture"));
   unload();

   /* Game B: QUICK SIGN IN with it. No password, and B has a separate copy. */
   play(second, GAME_B, info);
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   assert(!rib_achievements_quick_sign_in("Nobody"));
   assert(rib_achievements_quick_sign_in("Fixture"));
   ready();
   assert(!strcmp(snapshot().account, "Fixture"));
   assert(!rib_achievements_quick_sign_in("Fixture")); /* already signed in */
   snprintf(path, sizeof(path), "%s/achievements.session", second);
   assert((file = fopen(path, "rb")));
   assert(fgets(line, sizeof(line), file) && !strcmp(line, "Fixture\n"));
   assert(fgets(line, sizeof(line), file) && !strcmp(line, "fixture-token\n"));
   fclose(file);
   snprintf(path, sizeof(path), "%s/66697874757265/games/" GAME_B, accounts);
   assert(fs_exists(path));
   unload();

   /* The other way round: A, signed in with its password, signs out while B,
    * signed in with QUICK SIGN IN, still uses the account. It stays listed,
    * and A signs in with it again. */
   play(first, GAME_A, info);
   ready();
   rib_achievements_sign_out();
   assert(saved_accounts(saved) == 1);
   assert(rib_achievements_quick_sign_in("Fixture"));
   ready();
   unload();
   play(second, GAME_B, info);
   ready();

   /* B signs out. A still uses the account, so it stays listed. */
   rib_achievements_sign_out();
   assert(!fs_exists(path));
   assert(saved_accounts(saved) == 1);
   unload();

   /* When the player backs out of QUICK SIGN IN before the service answers,
    * we keep nothing: no session in this game, and turning achievements on
    * does not sign in with that account. */
   play(second, GAME_B, info);
   defer_login = true;
   assert(rib_achievements_quick_sign_in("Fixture"));
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNING_IN);
   rib_achievements_cancel();
   snprintf(path, sizeof(path), "%s/achievements.session", second);
   assert(!fs_exists(path));
   defer_login = false;
   {
      rc_api_server_response_t response = {0};
      response.body = login_json;
      response.body_length = strlen(login_json);
      response.http_status_code = 200;
      deferred_callback(&response, deferred_data);
   }
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   assert(!rib_achievements_set_enabled(true));
   unload();

   /* Choosing FORGET removes it from the list. A signs in by itself at its
    * next launch, which is not a new choice, so the account stays forgotten. */
   assert(rib_achievements_forget_account("Fixture"));
   assert(saved_accounts(saved) == 0);
   play(first, GAME_A, info);
   ready();
   assert(saved_accounts(saved) == 0);
   rib_achievements_sign_out();
   unload();

   /* When RetroAchievements rejects a session, we remove it from the list. */
   play(second, GAME_B, info);
   assert(rib_achievements_sign_in("Fixture", "fixture-password"));
   ready();
   assert(saved_accounts(saved) == 1);
   unload();
   expire_token = true;
   play(second, GAME_B, info);
   answered();
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   assert(saved_accounts(saved) == 0);
   expire_token = false;
   unload();

   /* A mistyped password in another game changes nothing on the list. */
   play(first, GAME_A, info);
   assert(rib_achievements_sign_in("Fixture", "fixture-password"));
   ready();
   unload();
   refuse_password = true;
   play(second, GAME_B, info);
   assert(rib_achievements_sign_in("Fixture", "wrong-password"));
   answered();
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   assert(saved_accounts(saved) == 1);
   refuse_password = false;
   unload();
   play(first, GAME_A, info);
   ready();
   rib_achievements_sign_out(); /* the last game using it */
   assert(saved_accounts(saved) == 0);
   unload();

   /* With achievements excluded, we list no saved accounts and use none. */
   play(first, GAME_A, info);
   assert(rib_achievements_sign_in("Fixture", "fixture-password"));
   ready();
   unload();
   test_setenv("ROMINABOX_ACHIEVEMENTS", "0");
   assert(saved_accounts(saved) == 0);
   assert(!rib_achievements_quick_sign_in("Fixture"));
   assert(!rib_achievements_forget_account("Fixture"));
   test_setenv("ROMINABOX_ACHIEVEMENTS", "1");
   play(first, GAME_A, info);
   ready();
   rib_achievements_sign_out();
   unload();

   /* Without the folder, a game signs in as before and we list nothing. */
   test_unsetenv("ROMINABOX_ACCOUNTS_DIR");
   play(second, GAME_B, info);
   assert(rib_achievements_sign_in("Fixture", "fixture-password"));
   ready();
   assert(saved_accounts(saved) == 0);
   rib_achievements_sign_out();
   unload();

   /* Everything left the list, so only the lock remains. */
   snprintf(path, sizeof(path), "%s/lock", accounts);
   assert(unlink(path) == 0);
   assert(rmdir(accounts) == 0);
   test_unsetenv("ROMINABOX_GAME_IDENTITY");
   test_setenv("ROMINABOX_DATA_DIR", first);
}

int main(void)
{
   char directory[512];
   char second_directory[512];
   char invalid_directory[512];
   char session_path[512];
   char file_data[256];
   FILE *file;
   struct retro_game_info info = {0};
   rib_achievement_row_t row;
   rib_achievement_unlock_t unlock;
   uint8_t *progress;
   size_t progress_size;
   const rc_client_achievement_t *achievement;
   char badge_name[8];
   uint32_t revision;
   unsigned requested;

   test_temporary_folder(directory, sizeof directory, "rib-achievements-runtime-");
   test_temporary_folder(second_directory, sizeof second_directory, "rib-achievements-other-");
   snprintf(invalid_directory, sizeof(invalid_directory), "%s/missing", directory);
   test_setenv("ROMINABOX_DATA_DIR", "relative-store");
   test_setenv("ROMINABOX_ACHIEVEMENTS", "1");
   info.path = "fixture.gbc";
   info.data = "fixture";
   info.size = 7;
   assert(!rib_achievements_content_load(&info));
   assert(snapshot().status == RIB_ACHIEVEMENTS_ERROR);
   test_setenv("ROMINABOX_DATA_DIR", directory);
   assert(rib_achievements_content_load(&info));
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);

   /* Cancel an actual pending rc_client login. Its late HTTP callback must not
    * reactivate the session. */
   defer_login = true;
   assert(rib_achievements_sign_in("Fixture", "fixture-password"));
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNING_IN);
   rib_achievements_cancel();
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   defer_login = false;
   {
      rc_api_server_response_t response = {0};
      response.body = login_json;
      response.body_length = strlen(login_json);
      response.http_status_code = 200;
      deferred_callback(&response, deferred_data);
      assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   }

   assert(rib_achievements_sign_in("Fixture", "fixture-password"));
   ready();
   assert(!rc_client_get_hardcore_enabled(locals.client));
   assert(snapshot().count == 1);
   achievement = rc_client_get_achievement_info(locals.client, 123);
   assert(achievement);
   strcpy(badge_name, achievement->badge_name);
   strcpy(((rc_client_achievement_t*)achievement)->badge_name, "../bad");
   assert(rib_achievements_get_row(0, &row));
   assert(badge_downloads == 0);
   strcpy(((rc_client_achievement_t*)achievement)->badge_name, badge_name);
   assert(rib_achievements_get_row(0, &row));
   assert(badge_downloads == 1);
   assert(row.id == 123 && row.state == RIB_ACHIEVEMENT_LOCKED);

   badge_lifecycle(directory);

   /* The per-game file contains only username, token and ON/OFF preference. */
   snprintf(session_path, sizeof(session_path), "%s/achievements.session", directory);
#ifndef _WIN32
   /* Only this user may read it: through its mode on macOS and Linux, and
    * through the access list of the folder on Windows. */
   {
      struct stat st;
      assert(stat(session_path, &st) == 0);
      assert((st.st_mode & 077) == 0);
   }
#endif
   file = fopen(session_path, "rb");
   assert(file);
   assert(fgets(file_data, sizeof(file_data), file));
   assert(strcmp(file_data, "Fixture\n") == 0);
   assert(fgets(file_data, sizeof(file_data), file));
   assert(strcmp(file_data, "fixture-token\n") == 0);
   assert(fgets(file_data, sizeof(file_data), file));
   assert(strcmp(file_data, "1\n") == 0);
   fclose(file);

   test_setenv("ROMINABOX_DATA_DIR", invalid_directory);
   assert(!rib_achievements_set_enabled(false));
   assert(snapshot().status == RIB_ACHIEVEMENTS_ERROR);
   assert(strstr(snapshot().error, "save") != NULL);
   test_setenv("ROMINABOX_DATA_DIR", directory);
   memory_byte = 1;
   rc_client_do_frame(locals.client);
   assert(awards == 0);
   assert(rib_achievements_set_enabled(false));
   assert(snapshot().status == RIB_ACHIEVEMENTS_OFF);
   memory_byte = 1;
   rc_client_idle(locals.client); /* exactly the managed OFF branch */
   assert(awards == 0 && !rib_achievements_has_unlocks());

   /* A state saved while earning is OFF still contains rc_client progress. */
   progress_size = rc_client_progress_size(locals.client);
   assert(progress_size > 0);
   progress = (uint8_t*)malloc(progress_size);
   assert(progress);
   assert(rc_client_serialize_progress(locals.client, progress) == RC_OK);
   assert(rc_client_deserialize_progress(locals.client, progress) == RC_OK);
   free(progress);

   assert(rib_achievements_set_enabled(true));
   assert(snapshot().status == RIB_ACHIEVEMENTS_ACTIVE);
   rc_client_do_frame(locals.client);
   rc_client_do_frame(locals.client);
   assert(awards == 0); /* stale hit count must not carry across OFF time */
   defer_award = true;
   requested = badge_downloads;
   rc_client_do_frame(locals.client);
   assert(awards == 1);
   assert(snapshot().pending_upload);
   /* At an unlock we request the colour badge at once, not when we paint a
    * row that may not be on screen. */
   assert(badge_downloads == requested + 1);
   assert(rib_achievements_has_unlocks());
   assert(rib_achievements_take_unlock(&unlock));
   assert(unlock.id == 123 && unlock.points == 5);
   assert(strcmp(unlock.title, "First step") == 0);
   assert(!unlock.badge_path[0]);
   assert(!rib_achievements_has_unlocks());
   assert(rib_achievements_get_row(0, &row));
   assert(row.state == RIB_ACHIEVEMENT_UNLOCKED);
   assert(row.badge == RIB_ACHIEVEMENT_BADGE_LOADING);
   assert(badge_downloads == requested + 1);
   colour_badge_arrives(directory);
   {
      rc_api_server_response_t response = {0};
      response.body = award_json;
      response.body_length = strlen(award_json);
      response.http_status_code = 200;
      defer_award = false;
      deferred_award_callback(&response, deferred_award_data);
      rib_achievements_award_request_finished(deferred_award_generation);
   }
   assert(!snapshot().pending_upload);
   assert(rib_achievements_set_enabled(false));
   assert(rib_achievements_set_enabled(true));
   assert(rib_achievements_get_row(0, &row));
   assert(row.state == RIB_ACHIEVEMENT_UNLOCKED && awards == 1);

   rib_achievements_content_unload();
   rc_client_destroy(locals.client);
   locals.client = NULL;
   memory_byte = 0;
   defer_login = true;
   assert(rib_achievements_content_load(&info));
   assert(rib_achievements_should_defer_restore());
   rib_achievements_begin_startup_gate();
   assert(snapshot().startup_waiting && !rib_achievements_startup_ready());
   assert(!rc_client_is_game_loaded(locals.client));
   /* A state load at this point would have no achievement progress target,
    * so we queue core frames and the restore in the runloop gate until ready. */
   defer_login = false;
   {
      rc_api_server_response_t response = {0};
      response.body = login_json;
      response.body_length = strlen(login_json);
      response.http_status_code = 200;
      deferred_callback(&response, deferred_data);
   }
   ready(); /* remembered token and ON preference */
   assert(snapshot().status == RIB_ACHIEVEMENTS_ACTIVE);
   assert(rib_achievements_startup_ready());
   rib_achievements_finish_startup_gate();
   assert(!snapshot().startup_waiting);

   /* After two failed award responses, a retry is scheduled in rc_client. The
    * managed status stays pending until the later acknowledgement. */
   fail_award_attempts = 2;
   memory_byte = 1;
   rc_client_do_frame(locals.client);
   rc_client_do_frame(locals.client);
   rc_client_do_frame(locals.client);
   rc_client_idle(locals.client);
   assert(snapshot().pending_upload);
   assert(rib_achievements_set_enabled(false));
   assert(snapshot().pending_upload);
   assert(rib_achievements_set_enabled(true));
   assert(snapshot().pending_upload); /* rc_client_reset keeps the retry */
   defer_award = true;
   fake_time = 3000;
   rc_client_idle(locals.client);
   assert(snapshot().pending_upload);
   {
      rc_api_server_response_t response = {0};
      response.body = award_json;
      response.body_length = strlen(award_json);
      response.http_status_code = 200;
      defer_award = false;
      deferred_award_callback(&response, deferred_award_data);
      rib_achievements_award_request_finished(deferred_award_generation);
   }
   rc_client_idle(locals.client);
   assert(!snapshot().pending_upload);
   assert(!snapshot().upload_failed);
   rc_client_do_frame(locals.client); /* dispatch pending mastery before unload */

   rib_achievements_content_unload();
   rc_client_destroy(locals.client);
   locals.client = NULL;
   fail_login = true;
   assert(rib_achievements_content_load(&info));
   rib_achievements_begin_startup_gate();
   assert(snapshot().status == RIB_ACHIEVEMENTS_ERROR);
   assert(snapshot().startup_waiting && !rib_achievements_startup_ready());
   fail_login = false;
   assert(rib_achievements_retry());
   ready();
   assert(rib_achievements_startup_ready());
   rib_achievements_finish_startup_gate();
   reject_award = true;
   rc_client_do_frame(locals.client);
   rc_client_do_frame(locals.client);
   rc_client_do_frame(locals.client);
   assert(snapshot().upload_failed);
   assert(strstr(snapshot().error, "Synthetic award rejection") != NULL);
   assert(!snapshot().pending_upload);
   reject_award = false;
   rib_achievements_sign_out();
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   assert(!snapshot().upload_failed && !snapshot().error[0]);
   replace_account = true;
   assert(rib_achievements_sign_in("Other", "replacement-password"));
   ready();
   assert(strcmp(snapshot().account, "Other") == 0);
   assert(!snapshot().upload_failed && !snapshot().error[0]);
   rib_achievements_content_unload();
   revision = snapshot().revision;
   rc_client_destroy(locals.client);
   locals.client = NULL;
   test_setenv("ROMINABOX_DATA_DIR", second_directory);
   assert(rib_achievements_content_load(&info));
   assert(snapshot().revision > revision);
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   assert(!snapshot().account[0] && !snapshot().enabled_preference);
   assert(!snapshot().upload_failed && !snapshot().error[0]);
   rib_achievements_content_unload();
   rc_client_destroy(locals.client);
   locals.client = NULL;
   test_setenv("ROMINABOX_DATA_DIR", directory);
   defer_login = true;
   assert(rib_achievements_content_load(&info));
   rib_achievements_begin_startup_gate();
   assert(snapshot().startup_waiting && !rib_achievements_startup_ready());
   rib_achievements_skip_startup();
   assert(rib_achievements_startup_ready());
   assert(snapshot().status == RIB_ACHIEVEMENTS_OFF);
   assert(snapshot().enabled_preference);
   assert(!rib_achievements_evaluating());
   file = fopen(session_path, "rb");
   assert(file);
   assert(fgets(file_data, sizeof(file_data), file));
   assert(strcmp(file_data, "Other\n") == 0);
   assert(fgets(file_data, sizeof(file_data), file));
   assert(strcmp(file_data, "other-token\n") == 0);
   assert(fgets(file_data, sizeof(file_data), file));
   assert(strcmp(file_data, "1\n") == 0);
   fclose(file);
   defer_login = false;
   {
      rc_api_server_response_t response = {0};
      response.body = login_json;
      response.body_length = strlen(login_json);
      response.http_status_code = 200;
      deferred_callback(&response, deferred_data);
   }
   assert(snapshot().status == RIB_ACHIEVEMENTS_OFF);
   assert(!rib_achievements_evaluating());
   rib_achievements_finish_startup_gate();
   assert(!snapshot().startup_waiting);
   rib_achievements_content_unload();
   rc_client_destroy(locals.client);
   locals.client = NULL;
   assert(rib_achievements_content_load(&info));
   ready(); /* the saved ON choice still restores on the next launch */
   assert(snapshot().enabled_preference);
   test_setenv("ROMINABOX_DATA_DIR", invalid_directory);
   rib_achievements_sign_out();
   assert(snapshot().status == RIB_ACHIEVEMENTS_ERROR);
   assert(strstr(snapshot().error, "remove") != NULL);
   test_setenv("ROMINABOX_DATA_DIR", directory);
   rib_achievements_sign_out();
   assert(snapshot().status == RIB_ACHIEVEMENTS_SIGNED_OUT);
   assert(!fs_exists(session_path));
   rib_achievements_content_unload();
   rc_client_destroy(locals.client);
   locals.client = NULL;
   shared_accounts(&info, directory, second_directory);
   {
      char badge_path[512];
      char badge_dir[512];
      snprintf(badge_path, sizeof(badge_path), "%s/achievements-badges/123_lock.png", directory);
      snprintf(badge_dir, sizeof(badge_dir), "%s/achievements-badges", directory);
      assert(unlink(badge_path) == 0);
      assert(rmdir(badge_dir) == 0);
   }
   assert(rmdir(directory) == 0);
   assert(rmdir(second_directory) == 0);
   puts("achievements runtime client integration passed");
   return 0;
}
