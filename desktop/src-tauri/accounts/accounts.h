/* The RetroAchievements accounts that the player can choose in QUICK SIGN IN
 * in every ROM-in-a-Box game. They are in one fixed folder, set in
 * ROMINABOX_ACCOUNTS_DIR by the launcher, and we never look anywhere else.
 *
 *   <folder>/lock                        locked while anything changes
 *   <folder>/<hex username>/session      username, display name, token
 *   <folder>/<hex username>/games/<id>   a game that uses the account
 *
 * Each game keeps its own session in its own storage, and the account signed
 * in there is the one in that session. This folder is a list to copy from. We
 * list an account while it has a session, and remove it when the player signs
 * out in the last game that uses it, its token is rejected, or it is forgotten.
 *
 * This is ROM-in-a-Box's own code. We link it into the player, and it does
 * not depend on RetroArch. */
#ifndef ROMINABOX_ACCOUNTS_H
#define ROMINABOX_ACCOUNTS_H

#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define RIB_ACCOUNTS_NAME_SIZE 96
#define RIB_ACCOUNTS_TOKEN_SIZE 128
#define RIB_ACCOUNTS_GAME_SIZE 24 /* hex digits of a game's identity */

typedef struct rib_saved_account {
   char username[RIB_ACCOUNTS_NAME_SIZE];
   char display_name[RIB_ACCOUNTS_NAME_SIZE];
   char token[RIB_ACCOUNTS_TOKEN_SIZE];
   long long used; /* the newest sign-in by any game, in seconds */
} rib_saved_account_t;

/* ROMINABOX_ACCOUNTS_DIR contains the path of an existing absolute folder. */
bool rib_accounts_available(void);
/* The player signed in as `username` in the game with identity `game`. Save
 * the account and record that this game uses it. */
bool rib_accounts_remember(const char *username, const char *display_name,
      const char *token, const char *game);
/* The player signed out in that game. Remove the account if no game uses it. */
bool rib_accounts_forget(const char *username, const char *game);
/* `token` is no longer valid. Remove the account only if it still has that
 * token, because a newer one may have been saved in another game. */
bool rib_accounts_drop_if(const char *username, const char *token);
/* The player removed the account from the list, for every game. */
bool rib_accounts_erase(const char *username);
/* Up to `capacity` accounts, most recently used first. */
size_t rib_accounts_list(rib_saved_account_t *out, size_t capacity);

#ifdef __cplusplus
}
#endif

#endif
