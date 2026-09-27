/* The shared accounts store (desktop/src-tauri/accounts), on actual files in
 * a temporary folder. No network, no account, no window.
 *
 * In the last case we run several processes against one folder at once and
 * then check that nothing was lost and nothing was left half-written.
 *
 * We note what differs by platform where it differs: how we make a temporary
 * folder and a second process, and how we keep an account private (the
 * folder modes on macOS and Linux, a token sealed to the user on Windows). */
#include "accounts.h"
#include "sealed.h"
#include "../launcher/accounts_folder.h"
#include "../launcher/portable_fs.h"

#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <utime.h>

#ifdef _WIN32
#include <direct.h>
#include <io.h>
#include <process.h>
#define F_OK 0
#else
#include <sys/wait.h>
#include <unistd.h>
#endif

#define GAME_A "aaaaaaaaaaaaaaaaaaaaaaaa"
#define GAME_B "bbbbbbbbbbbbbbbbbbbbbbbb"

static char root[1024];
static char folder[1100];
static const char *self;

static void set_variable(const char *name, const char *value)
{
#ifdef _WIN32
   /* An empty value removes the variable. */
   assert(_putenv_s(name, value ? value : "") == 0);
#else
   assert(value ? setenv(name, value, 1) == 0 : unsetenv(name) == 0);
#endif
}

static void make_folder(const char *path)
{
#ifdef _WIN32
   assert(_mkdir(path) == 0);
#else
   assert(mkdir(path, 0700) == 0);
#endif
}

static size_t listed(rib_saved_account_t *accounts, size_t capacity)
{
   return rib_accounts_list(accounts, capacity);
}

static int count_entry(const char *name, void *count)
{
   (void)name;
   ++*(int*)count;
   return 0;
}

/* Every entry that we could have made in the store, whose names never start
 * with a dot. -1 when the folder is not there. */
static int entries(const char *path)
{
   int count = 0;
   if (fs_list(path, count_entry, &count) != 0)
      return -1;
   return count;
}

static void path_of(char *out, size_t size, const char *relative)
{
   snprintf(out, size, "%s/%s", folder, relative);
}

#ifndef _WIN32
static mode_t mode_of(const char *relative)
{
   char path[1400];
   struct stat info;
   path_of(path, sizeof path, relative);
   assert(stat(path, &info) == 0);
   return info.st_mode & 0777;
}
#endif

/* Sets when a game last signed in, so ordering does not depend on the clock. */
static void signed_in_at(const char *key, const char *game, long seconds)
{
   char path[1400];
   char relative[512];
   struct utimbuf times = {seconds, seconds};
   snprintf(relative, sizeof relative, "%s/games/%s", key, game);
   path_of(path, sizeof path, relative);
   assert(utime(path, &times) == 0);
}

/* Each case starts in a folder of its own inside this run's temporary one. */
static void reset(void)
{
   static int cases;
   snprintf(folder, sizeof folder, "%s/accounts-%d", root, ++cases);
   make_folder(folder);
   set_variable("ROMINABOX_ACCOUNTS_DIR", folder);
}

/* Everything under `path`, which belongs to this run. We make no link here,
 * and we never follow one in the file layer. */
static int remove_entry(const char *name, void *parent)
{
   char path[1400];
   snprintf(path, sizeof path, "%s/%s", (const char*)parent, name);
   if (fs_is_directory(path))
   {
      assert(fs_list(path, remove_entry, path) == 0);
      return fs_remove_directory(path);
   }
   return fs_remove(path);
}

/* Only the folder we made for this run, checked just before. */
static void remove_run_folder(void)
{
   const char *name = strrchr(root, '/');
   assert(name && !strncmp(name, "/rominabox-accounts-test-", 25) && strlen(name) == 31);
   assert(fs_is_directory(root));
#ifndef _WIN32
   {
      struct stat info;
      assert(lstat(root, &info) == 0 && S_ISDIR(info.st_mode) && info.st_uid == getuid());
   }
#endif
   assert(fs_list(root, remove_entry, root) == 0);
   assert(fs_remove_directory(root) == 0);
}

/* The token of a session, as we read it back from the store: sealed to this
 * user on Windows, as given elsewhere. */
static bool stored_token(const char *text, const char *names, char *plain, size_t capacity)
{
   char stored[2048];
   const char *token, *end;
   if (strncmp(text, names, strlen(names)))
      return false;
   token = text + strlen(names);
   end = strchr(token, '\n');
   if (!end || end[1] || (size_t)(end - token) >= sizeof stored)
      return false;
   memcpy(stored, token, (size_t)(end - token));
   stored[end - token] = '\0';
   return rib_unseal(stored, plain, capacity);
}

/* "joao" in hex: the folder for an account named joao, JOAO or Joao. */
#define JOAO "6a6f616f"
#define KID "6b6964"

static void unavailable_without_a_named_folder(void)
{
   rib_saved_account_t accounts[4];
   set_variable("ROMINABOX_ACCOUNTS_DIR", NULL);
   assert(!rib_accounts_available());
   assert(!rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(listed(accounts, 4) == 0);
   set_variable("ROMINABOX_ACCOUNTS_DIR", "relative/accounts");
   assert(!rib_accounts_available());
   set_variable("ROMINABOX_ACCOUNTS_DIR", "/nonexistent/rominabox-accounts");
   assert(!rib_accounts_available());
   assert(!rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   set_variable("ROMINABOX_ACCOUNTS_DIR", folder);
   assert(rib_accounts_available());
}

#ifdef _WIN32
/* A folder named like a share, \\server\name, is as absolute as one under a
 * drive letter. In the launcher we make the accounts folder in a per-user
 * folder that may be on a share, and we must accept it in the store. \\?\ is
 * the local form of such a name. */
static void a_share_path_names_the_folder(void)
{
   char share[1400];
   size_t index;
   reset();
   snprintf(share, sizeof share, "\\\\?\\%s", folder);
   for (index = 0; share[index]; ++index)
      if (share[index] == '/')
         share[index] = '\\';
   set_variable("ROMINABOX_ACCOUNTS_DIR", share);
   assert(rib_accounts_available());
   set_variable("ROMINABOX_ACCOUNTS_DIR", folder);
}
#endif

static void a_sign_in_is_listed_privately(void)
{
   rib_saved_account_t accounts[4];
   char session[1400];
   char text[512] = {0};
   char plain[256];
   FILE *file;
   reset();
   assert(rib_accounts_remember("joao", "Joao", "token-1", GAME_A));
   assert(listed(accounts, 4) == 1);
   assert(!strcmp(accounts[0].username, "joao"));
   assert(!strcmp(accounts[0].display_name, "Joao"));
   assert(!strcmp(accounts[0].token, "token-1"));
   path_of(session, sizeof session, JOAO "/session");
   file = fopen(session, "rb");
   assert(file && fread(text, 1, sizeof text - 1, file) > 0);
   fclose(file);
   assert(stored_token(text, "joao\nJoao\n", plain, sizeof plain) && !strcmp(plain, "token-1"));
#ifdef _WIN32
   /* Sealed to this Windows user: the file does not contain the token. */
   assert(!strstr(text, "token-1"));
#else
   /* The folder's modes are the protection, and the token is as given. */
   assert(!strcmp(text, "joao\nJoao\ntoken-1\n"));
   assert(mode_of(JOAO) == 0700);
   assert(mode_of(JOAO "/games") == 0700);
   assert(mode_of(JOAO "/session") == 0600);
   assert(mode_of(JOAO "/games/" GAME_A) == 0600);
   assert(mode_of("lock") == 0600);
#endif
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
   make_folder(app_data);
   assert(rominabox_accounts_folder(app_data, "ROM-in-a-Box Accounts", out, sizeof out) == 0);
   snprintf(expected, sizeof expected, "%s/ROM-in-a-Box Accounts", app_data);
   assert(!strcmp(out, expected));
   assert(fs_is_directory(out));
#ifndef _WIN32
   {
      struct stat info;
      assert(stat(out, &info) == 0 && (info.st_mode & 0777) == 0700);
   }
#endif
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
   char token[64];
   int round;
   game_of(index, game);
   for (round = 0; round < ROUNDS; ++round)
   {
      /* The token contains its length, so part of one and part of another
       * cannot pass for a token. */
      {
         int length = 12 + (index * 7 + round) % 40, at;
         at = snprintf(token, sizeof token, "t%02d.", length);
         for (; at < length - 1; ++at)
            token[at] = (char)('a' + (index + round + at) % 26);
         token[at++] = '#';
         token[at] = '\0';
      }
      if (!rib_accounts_remember(names[index % 4], names[index % 4], token, game))
      {
         fprintf(stderr, "writer %d, round %d: signing in failed: %s\n", index, round, strerror(errno));
         _exit(10);
      }
      if ((round < ROUNDS - 1 || index % 2 == 0) && !rib_accounts_forget(names[index % 4], game))
      {
         fprintf(stderr, "writer %d, round %d: signing out failed: %s\n", index, round, strerror(errno));
         _exit(11);
      }
   }
   _exit(0);
}

/* A session file exactly as a writer finished it: the name twice and a token
 * as long as its stated length. */
static bool whole_session(const char *text, const char *name)
{
   char expected[64];
   char token[256];
   int length;
   snprintf(expected, sizeof expected, "%s\n%s\n", name, name);
   if (!stored_token(text, expected, token, sizeof token) || sscanf(token, "t%2d.", &length) != 1)
      return false;
   return (int)strlen(token) == length && token[length - 1] == '#';
}

/* Read every session directly from disk while the writers run, as when
 * another game lists the accounts: through the file layer, without the lock.
 * A file that exists must always be whole, and a writer must never fail
 * because we are reading the file. */
static void reader(void)
{
   int pass, key;
   for (pass = 0; pass < 4000; ++pass)
      for (key = 0; key < 4; ++key)
      {
         char path[1400], relative[64], text[4096];
         FILE *file;
         size_t size;
         snprintf(relative, sizeof relative, "%s/session", keys[key]);
         path_of(path, sizeof path, relative);
         if (!(file = fs_open(path, "rb")))
            continue;
         size = fread(text, 1, sizeof text - 1, file);
         fclose(file);
         text[size] = '\0';
         if (!whole_session(text, names[key]))
            _exit(20);
      }
   _exit(0);
}

/* Process `index` of a case: a writer, or the reader when it is WRITERS. On
 * macOS and Linux it is this process forked. On Windows, where there is no
 * fork, it is this program started again with the index, and it reads the
 * folder from the environment it inherits. */
static intptr_t start_process(int index)
{
#ifdef _WIN32
   char argument[16];
   intptr_t process;
   snprintf(argument, sizeof argument, "%d", index);
   process = _spawnl(_P_NOWAIT, self, self, "--process", argument, NULL);
   assert(process != -1);
   return process;
#else
   pid_t child = fork();
   assert(child >= 0);
   if (child == 0)
   {
      if (index == WRITERS)
         reader();
      writer(index);
   }
   return child;
#endif
}

/* Its exit code, or -1 when it did not exit on its own. */
static int finish_process(intptr_t process)
{
   int status;
#ifdef _WIN32
   assert(_cwait(&status, process, 0) == process);
   return status;
#else
   assert(waitpid((pid_t)process, &status, 0) == (pid_t)process);
   return WIFEXITED(status) ? WEXITSTATUS(status) : -1;
#endif
}

static void games_at_the_same_time_lose_nothing(void)
{
   intptr_t children[WRITERS + 1];
   rib_saved_account_t accounts[8];
   int index, key;
   reset();
   for (index = 0; index <= WRITERS; ++index)
      children[index] = start_process(index);
   for (index = 0; index <= WRITERS; ++index)
   {
      const int code = finish_process(children[index]);
      if (code != 0)
      {
         fprintf(stderr, "process %d failed with %d\n", index, code);
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
   self = argv[0];
   /* A writer or the reader of the case above, started again on Windows. */
   if (argc == 3 && !strcmp(argv[1], "--process"))
   {
      const int index = atoi(argv[2]);
      const char *named = getenv("ROMINABOX_ACCOUNTS_DIR");
      assert(named && strlen(named) < sizeof folder);
      strcpy(folder, named);
      if (index == WRITERS)
         reader();
      writer(index);
   }
   /* The folder to work in, from the runner: work/test-output. */
   assert(argc == 2);
   snprintf(root, sizeof root, "%s/rominabox-accounts-test-XXXXXX", argv[1]);
#ifdef _WIN32
   assert(_mktemp_s(root, strlen(root) + 1) == 0);
   make_folder(root);
#else
   assert(argv[1][0] == '/' && mkdtemp(root));
#endif
   reset();

   unavailable_without_a_named_folder();
#ifdef _WIN32
   a_share_path_names_the_folder();
#endif
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
