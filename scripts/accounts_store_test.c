/* The shared accounts store (desktop/src-tauri/accounts), on actual files in
 * a temporary folder. No network, no account, no window.
 *
 * In the last case we run several processes against one folder at once and
 * then check that nothing was lost and nothing was left half-written. */
#include "accounts.h"
#include "../launcher/accounts_folder.h"

#include <assert.h>
#include <dirent.h>
#include <errno.h>
#include <ftw.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/wait.h>
#include <unistd.h>

#define GAME_A "aaaaaaaaaaaaaaaaaaaaaaaa"
#define GAME_B "bbbbbbbbbbbbbbbbbbbbbbbb"

static char root[1024];
static char folder[1100];

static size_t listed(rib_saved_account_t *accounts, size_t capacity)
{
   return rib_accounts_list(accounts, capacity);
}

static int entries(const char *path)
{
   DIR *directory = opendir(path);
   struct dirent *entry;
   int count = 0;
   if (!directory)
      return -1;
   while ((entry = readdir(directory)))
      if (strcmp(entry->d_name, ".") && strcmp(entry->d_name, ".."))
         count++;
   closedir(directory);
   return count;
}

static void path_of(char *out, size_t size, const char *relative)
{
   snprintf(out, size, "%s/%s", folder, relative);
}

static mode_t mode_of(const char *relative)
{
   char path[1400];
   struct stat info;
   path_of(path, sizeof path, relative);
   assert(stat(path, &info) == 0);
   return info.st_mode & 0777;
}

/* Sets when a game last signed in, so ordering does not depend on the clock. */
static void signed_in_at(const char *key, const char *game, long seconds)
{
   char path[1400];
   char relative[512];
   struct timeval times[2] = {{seconds, 0}, {seconds, 0}};
   snprintf(relative, sizeof relative, "%s/games/%s", key, game);
   path_of(path, sizeof path, relative);
   assert(utimes(path, times) == 0);
}

/* Each case starts in a folder of its own inside this run's temporary one. */
static void reset(void)
{
   static int cases;
   snprintf(folder, sizeof folder, "%s/accounts-%d", root, ++cases);
   assert(mkdir(folder, 0700) == 0);
   setenv("ROMINABOX_ACCOUNTS_DIR", folder, 1);
}

static int remove_entry(const char *path, const struct stat *info, int kind, struct FTW *walk)
{
   (void)info;
   (void)walk;
   return kind == FTW_DP ? rmdir(path) : unlink(path);
}

/* Only the folder we made with mkdtemp for this run, checked just before. */
static void remove_run_folder(void)
{
   struct stat info;
   const char *name = strrchr(root, '/');
   assert(name && !strncmp(name, "/rominabox-accounts-test-", 25) && strlen(name) == 31);
   assert(lstat(root, &info) == 0 && S_ISDIR(info.st_mode) && info.st_uid == getuid());
   assert(nftw(root, remove_entry, 16, FTW_DEPTH | FTW_PHYS) == 0);
}

/* "joao" in hex: the folder for an account named joao, JOAO or Joao. */
#define JOAO "6a6f616f"
#define KID "6b6964"

static void unavailable_without_a_named_folder(void)
{
   rib_saved_account_t accounts[4];
   unsetenv("ROMINABOX_ACCOUNTS_DIR");
   assert(!rib_accounts_available());
   assert(!rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(listed(accounts, 4) == 0);
   setenv("ROMINABOX_ACCOUNTS_DIR", "relative/accounts", 1);
   assert(!rib_accounts_available());
   setenv("ROMINABOX_ACCOUNTS_DIR", "/nonexistent/rominabox-accounts", 1);
   assert(!rib_accounts_available());
   assert(!rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   setenv("ROMINABOX_ACCOUNTS_DIR", folder, 1);
   assert(rib_accounts_available());
}

static void a_sign_in_is_listed_privately(void)
{
   rib_saved_account_t accounts[4];
   char session[1400];
   char text[512] = {0};
   FILE *file;
   reset();
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(listed(accounts, 4) == 1);
   assert(!strcmp(accounts[0].username, "joao"));
   assert(!strcmp(accounts[0].display_name, "Joao"));
   assert(!strcmp(accounts[0].token, "token-1"));
   assert(mode_of(JOAO) == 0700);
   assert(mode_of(JOAO "/games") == 0700);
   assert(mode_of(JOAO "/session") == 0600);
   assert(mode_of(JOAO "/games/" GAME_A) == 0600);
   assert(mode_of("lock") == 0600);
   path_of(session, sizeof session, JOAO "/session");
   file = fopen(session, "rb");
   assert(file && fread(text, 1, sizeof text - 1, file) > 0);
   fclose(file);
   assert(!strcmp(text, "joao\nJoao\ntoken-1\n"));
}

static void different_accounts_live_side_by_side(void)
{
   rib_saved_account_t accounts[4];
   reset();
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(rib_accounts_remember("kid", "Kid", "token-2", GAME_B));
   signed_in_at(JOAO, GAME_A, 1000);
   signed_in_at(KID, GAME_B, 2000);
   assert(listed(accounts, 4) == 2);
   assert(!strcmp(accounts[0].username, "kid") && !strcmp(accounts[1].username, "joao"));
   assert(accounts[0].used == 2000 && accounts[1].used == 1000);
   signed_in_at(JOAO, GAME_A, 3000);
   assert(listed(accounts, 4) == 2 && !strcmp(accounts[0].username, "joao"));
   /* Only as many as asked for, and those are the newest. */
   assert(listed(accounts, 1) == 1 && !strcmp(accounts[0].username, "joao"));
}

static void one_name_in_any_case_is_one_account(void)
{
   rib_saved_account_t accounts[4];
   reset();
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(rib_accounts_remember("JOAO", "Joao", "token-2", GAME_B));
   assert(listed(accounts, 4) == 1);
   assert(!strcmp(accounts[0].token, "token-2"));
   assert(entries(folder) == 2); /* the lock and one account */
}

static void the_last_game_to_sign_out_removes_it(void)
{
   rib_saved_account_t accounts[4];
   char account[1400];
   reset();
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_B));
   assert(rib_accounts_forget("joao", GAME_A));
   assert(listed(accounts, 4) == 1);
   assert(rib_accounts_forget("joao", GAME_A)); /* already gone: still fine */
   assert(listed(accounts, 4) == 1);
   assert(rib_accounts_forget("joao", GAME_B));
   assert(listed(accounts, 4) == 0);
   path_of(account, sizeof account, JOAO);
   assert(access(account, F_OK) != 0 && errno == ENOENT);
   /* Signing out of an account nobody saved is not an error. */
   assert(rib_accounts_forget("kid", GAME_A));
}

static void a_rejected_token_goes_only_while_it_is_the_saved_one(void)
{
   rib_saved_account_t accounts[4];
   reset();
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(rib_accounts_remember("joao", "Joao", "token-2", GAME_B));
   assert(rib_accounts_drop_if("joao", "token-1"));
   assert(listed(accounts, 4) == 1 && !strcmp(accounts[0].token, "token-2"));
   assert(rib_accounts_drop_if("joao", "token-2"));
   assert(listed(accounts, 4) == 0);
   assert(entries(folder) == 1); /* only the lock */
}

static void forget_removes_it_for_every_game(void)
{
   rib_saved_account_t accounts[4];
   reset();
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_B));
   assert(rib_accounts_remember("kid", "Kid", "token-2", GAME_B));
   assert(rib_accounts_erase("joao"));
   assert(listed(accounts, 4) == 1 && !strcmp(accounts[0].username, "kid"));
   /* A game still signed in as joao signs out later: nothing breaks. */
   assert(rib_accounts_forget("joao", GAME_A));
   assert(listed(accounts, 4) == 1);
}

static void a_name_never_becomes_a_path(void)
{
   rib_saved_account_t accounts[4];
   reset();
   assert(rib_accounts_remember("../../escape", "Escape", "token-1", GAME_A));
   assert(rib_accounts_remember("a/b\\c", "Slash", "token-2", GAME_A));
   assert(entries(folder) == 3);
   assert(access("/tmp/escape", F_OK) != 0);
   assert(listed(accounts, 4) == 2);
   assert(!rib_accounts_remember("two\nlines", "Joao", "token-1", GAME_A));
   assert(!rib_accounts_remember("joao", "Two\nlines", "token-1", GAME_A));
   assert(!rib_accounts_remember("joao", "Joao", "token\n2", GAME_A));
   assert(!rib_accounts_remember("", "Joao", "token-1", GAME_A));
   assert(!rib_accounts_remember("joao", "Joao", "", GAME_A));
   assert(!rib_accounts_remember("joao", "Joao", "token-1", "../games"));
   assert(!rib_accounts_remember("joao", "Joao", "token-1", "AAAAAAAAAAAAAAAAAAAAAAAA"));
   assert(listed(accounts, 4) == 2);
}

static void a_damaged_session_is_not_listed(void)
{
   rib_saved_account_t accounts[4];
   char session[1400];
   FILE *file;
   reset();
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   path_of(session, sizeof session, JOAO "/session");
   file = fopen(session, "wb");
   assert(file && fputs("joao\nJoao\n", file) >= 0);
   fclose(file);
   assert(listed(accounts, 4) == 0);
   /* Signing in again repairs it. */
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(listed(accounts, 4) == 1);
}

/* The launcher part: we make only the folder with exactly the name from the
 * export, private inside the application-data folder. */
static void the_launcher_makes_exactly_the_named_folder(void)
{
   char app_data[1200], out[1400], expected[1400];
   reset();
   snprintf(app_data, sizeof app_data, "%s/Application Support", folder);
   assert(mkdir(app_data, 0755) == 0);
   assert(rominabox_accounts_folder(app_data, "ROM-in-a-Box Accounts", out, sizeof out) == 0);
   snprintf(expected, sizeof expected, "%s/ROM-in-a-Box Accounts", app_data);
   assert(!strcmp(out, expected));
   {
      struct stat info;
      assert(stat(out, &info) == 0 && S_ISDIR(info.st_mode) && (info.st_mode & 0777) == 0700);
   }
   /* At the second launch the folder is already there. */
   assert(rominabox_accounts_folder(app_data, "ROM-in-a-Box Accounts", out, sizeof out) == 0);
   assert(rominabox_accounts_folder(app_data, "../escape", out, sizeof out) != 0);
   assert(rominabox_accounts_folder(app_data, "a/b", out, sizeof out) != 0);
   assert(rominabox_accounts_folder(app_data, ".hidden", out, sizeof out) != 0);
   assert(rominabox_accounts_folder(app_data, "", out, sizeof out) != 0);
   assert(rominabox_accounts_folder("/nonexistent/app-data", "ROM-in-a-Box Accounts", out, sizeof out) != 0);
   assert(rominabox_accounts_folder(app_data, "ROM-in-a-Box Accounts", out, 8) != 0);
   assert(entries(app_data) == 1);
}

/* ---- games at the same time ---------------------------------------------- */

#define WRITERS 8
#define ROUNDS 120

static const char *const names[] = {"alpha", "bravo", "charlie", "delta"};
static const char *const keys[] = {"616c706861", "627261766f", "636861726c6965", "64656c7461"};

static void game_of(int writer, char game[25])
{
   snprintf(game, 25, "%024x", writer + 1);
}

/* In each writer we sign in and out of one account over and over. Odd ones
 * end signed in and even ones signed out, so we know the end state. */
static void writer(int index)
{
   char game[25];
   char token[32];
   int round;
   game_of(index, game);
   for (round = 0; round < ROUNDS; ++round)
   {
      snprintf(token, sizeof token, "token-%d-%d", index, round);
      if (!rib_accounts_remember(names[index % 4], names[index % 4], token, game))
         _exit(10);
      if ((round < ROUNDS - 1 || index % 2 == 0) && !rib_accounts_forget(names[index % 4], game))
         _exit(11);
   }
   _exit(0);
}

/* Read every session directly from disk while the writers run. A file that
 * exists must always be whole. */
static void reader(void)
{
   int pass, key;
   for (pass = 0; pass < 4000; ++pass)
      for (key = 0; key < 4; ++key)
      {
         char path[1400], relative[64], text[256];
         FILE *file;
         size_t size;
         snprintf(relative, sizeof relative, "%s/session", keys[key]);
         path_of(path, sizeof path, relative);
         if (!(file = fopen(path, "rb")))
            continue;
         size = fread(text, 1, sizeof text - 1, file);
         fclose(file);
         text[size] = '\0';
         if (size == 0 || text[size - 1] != '\n' || strncmp(text, names[key], strlen(names[key])))
            _exit(20);
      }
   _exit(0);
}

static void games_at_the_same_time_lose_nothing(void)
{
   pid_t children[WRITERS + 1];
   rib_saved_account_t accounts[8];
   int index, status, key;
   reset();
   for (index = 0; index <= WRITERS; ++index)
   {
      children[index] = fork();
      assert(children[index] >= 0);
      if (children[index] == 0)
      {
         if (index == WRITERS)
            reader();
         writer(index);
      }
   }
   for (index = 0; index <= WRITERS; ++index)
   {
      assert(waitpid(children[index], &status, 0) == children[index]);
      if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
      {
         fprintf(stderr, "process %d failed with %d\n", index, WIFEXITED(status) ? WEXITSTATUS(status) : -1);
         assert(0);
      }
   }
   /* Odd writers 1, 3, 5, 7 use bravo and delta and stay signed in; alpha and
    * charlie are used only by writers that signed out. */
   assert(listed(accounts, 8) == 2);
   for (key = 0; key < 4; ++key)
   {
      char path[1400], relative[64];
      snprintf(relative, sizeof relative, "%s/games", keys[key]);
      path_of(path, sizeof path, relative);
      assert(entries(path) == (key % 2 ? 2 : -1));
   }
   assert(entries(folder) == 3); /* the lock, bravo and delta */
}

int main(int argc, char **argv)
{
   /* The folder to work in, from the runner: work/test-output. */
   assert(argc == 2 && argv[1][0] == '/');
   snprintf(root, sizeof root, "%s/rominabox-accounts-test-XXXXXX", argv[1]);
   assert(mkdtemp(root));
   reset();

   unavailable_without_a_named_folder();
   a_sign_in_is_listed_privately();
   different_accounts_live_side_by_side();
   one_name_in_any_case_is_one_account();
   the_last_game_to_sign_out_removes_it();
   a_rejected_token_goes_only_while_it_is_the_saved_one();
   forget_removes_it_for_every_game();
   a_name_never_becomes_a_path();
   a_damaged_session_is_not_listed();
   the_launcher_makes_exactly_the_named_folder();
   games_at_the_same_time_lose_nothing();

   remove_run_folder();
   puts("accounts store: 11 cases passed");
   return 0;
}
