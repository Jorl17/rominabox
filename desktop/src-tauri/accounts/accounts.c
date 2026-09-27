/* See accounts.h. We make every change under the folder's lock and replace
 * every file whole, so no change is lost when games run at the same time,
 * and nobody reads half a file. */
#include "accounts.h"
#include "sealed.h"
#include "../launcher/portable_fs.h"
/* The player's root, on the include path of every build. We compile a copy
 * of this file in the player build, so a path relative to it would break. */
#include "rominabox_launch.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define PATH_SIZE 4096
#define KEY_SIZE (RIB_ACCOUNTS_NAME_SIZE * 2)
#define SEALED_SIZE 1024

static const char *folder(void) {
    const char *path = getenv(RIB_ENV_ACCOUNTS_DIR);
    if (!path || !*path || !fs_is_absolute(path) || !fs_is_directory(path))
        return NULL;
    return path;
}

bool rib_accounts_available(void) {
    return folder() != NULL;
}

static bool join(char out[PATH_SIZE], const char *left, const char *right) {
    int length = snprintf(out, PATH_SIZE, "%s/%s", left, right);
    return length > 0 && length < PATH_SIZE;
}

static bool field_valid(const char *text, size_t capacity) {
    size_t length = text ? strlen(text) : 0;
    return length && length < capacity && !strpbrk(text, "\r\n");
}

/* RetroAchievements user names are not case-sensitive, so ABC and abc are
 * one account. We name the folder with the name in hex, so no name can point
 * outside the folder or match another on a case-insensitive disk. */
static bool key_for(const char *username, char key[KEY_SIZE + 1]) {
    static const char digits[] = "0123456789abcdef";
    size_t index;
    if (!field_valid(username, RIB_ACCOUNTS_NAME_SIZE))
        return false;
    for (index = 0; username[index]; ++index) {
        unsigned char c = (unsigned char)username[index];
        if (c >= 'A' && c <= 'Z')
            c = (unsigned char)(c - 'A' + 'a');
        key[index * 2] = digits[c >> 4];
        key[index * 2 + 1] = digits[c & 15];
    }
    key[index * 2] = '\0';
    return true;
}

static bool key_valid(const char *name) {
    size_t length = strlen(name), index;
    if (!length || length % 2 || length > KEY_SIZE)
        return false;
    for (index = 0; index < length; ++index)
        if (!((name[index] >= '0' && name[index] <= '9') || (name[index] >= 'a' && name[index] <= 'f')))
            return false;
    return true;
}

/* A game's identity as we write it in the exporter, 24 lowercase hex digits.
 * The name of a temporary file left by an interrupted write never matches. */
static bool game_valid(const char *game) {
    size_t index;
    if (!game || strlen(game) != RIB_ACCOUNTS_GAME_SIZE)
        return false;
    for (index = 0; index < RIB_ACCOUNTS_GAME_SIZE; ++index)
        if (!((game[index] >= '0' && game[index] <= '9') || (game[index] >= 'a' && game[index] <= 'f')))
            return false;
    return true;
}

typedef struct paths {
    char account[PATH_SIZE];
    char session[PATH_SIZE];
    char games[PATH_SIZE];
} paths_t;

static bool paths_for(const char *root, const char *key, paths_t *paths) {
    return join(paths->account, root, key) && join(paths->session, paths->account, "session")
        && join(paths->games, paths->account, "games");
}

static bool lock_folder(const char *root, fs_lock *lock) {
    char path[PATH_SIZE];
    lock->handle = 0;
    return join(path, root, "lock") && fs_lock_acquire(path, lock) == 0;
}

/* The whole file, or false when it is missing or larger than `capacity`. */
static bool read_whole(const char *path, char *out, size_t capacity) {
    FILE *stream = fs_open(path, "rb");
    size_t got;
    bool whole;
    if (!stream)
        return false;
    got = fread(out, 1, capacity - 1, stream);
    if (ferror(stream))
        whole = false;
    else if (got < capacity - 1)
        whole = true;
    else
        whole = fgetc(stream) == EOF && !ferror(stream);
    fclose(stream);
    if (!whole)
        return false;
    out[got] = '\0';
    return true;
}

static bool read_session(const char *path, rib_saved_account_t *account) {
    char text[RIB_ACCOUNTS_NAME_SIZE * 2 + SEALED_SIZE + 8];
    char *username = text, *display, *sealed, *end;
    memset(account, 0, sizeof *account);
    if (!read_whole(path, text, sizeof text))
        return false;
    if (!(display = strchr(username, '\n')))
        return false;
    *display++ = '\0';
    if (!(sealed = strchr(display, '\n')))
        return false;
    *sealed++ = '\0';
    if (!(end = strchr(sealed, '\n')) || end[1])
        return false;
    *end = '\0';
    if (!field_valid(username, sizeof account->username)
        || !field_valid(display, sizeof account->display_name)
        || !rib_unseal(sealed, account->token, sizeof account->token)
        || !field_valid(account->token, sizeof account->token)) {
        memset(account, 0, sizeof *account);
        return false;
    }
    strcpy(account->username, username);
    strcpy(account->display_name, display);
    return true;
}

static int any_game(const char *name, void *context) {
    if (!game_valid(name))
        return 0;
    *(bool *)context = true;
    return 1;
}

typedef struct newest {
    const char *games;
    long long used;
} newest_t;

static int newest_game(const char *name, void *context) {
    newest_t *newest = context;
    char path[PATH_SIZE];
    long long used;
    if (game_valid(name) && join(path, newest->games, name) && (used = fs_modified(path)) > newest->used)
        newest->used = used;
    return 0;
}

static int remove_game(const char *name, void *context) {
    char path[PATH_SIZE];
    if (game_valid(name) && join(path, context, name))
        fs_remove(path);
    return 0;
}

/* Under the lock. We remove only names that we write in this store. We keep
 * the folder of anything else that someone put there, and do not list it. */
static bool erase_locked(const paths_t *paths) {
    bool removed;
    if (fs_is_directory(paths->games))
        fs_list(paths->games, remove_game, (void *)paths->games);
    removed = fs_remove(paths->session) == 0;
    fs_remove_directory(paths->games);
    fs_remove_directory(paths->account);
    return removed;
}

bool rib_accounts_remember(const char *username, const char *display_name, const char *token,
                           const char *game) {
    const char *root = folder();
    char key[KEY_SIZE + 1];
    char sealed[SEALED_SIZE];
    char text[SEALED_SIZE + RIB_ACCOUNTS_NAME_SIZE * 2 + 8];
    char marker[PATH_SIZE];
    paths_t paths;
    fs_lock lock;
    int length;
    bool saved;
    if (!root || !key_for(username, key) || !field_valid(display_name, RIB_ACCOUNTS_NAME_SIZE)
        || !field_valid(token, RIB_ACCOUNTS_TOKEN_SIZE) || !game_valid(game)
        || !paths_for(root, key, &paths) || !join(marker, paths.games, game)
        || !rib_seal(token, sealed, sizeof sealed))
        return false;
    length = snprintf(text, sizeof text, "%s\n%s\n%s\n", username, display_name, sealed);
    if (length <= 0 || length >= (int)sizeof text || !lock_folder(root, &lock))
        return false;
    saved = fs_make_private_directory(paths.account) == 0
        && fs_make_private_directory(paths.games) == 0
        && fs_write_file(paths.session, text, (size_t)length) == 0
        && fs_write_file(marker, "", 0) == 0;
    fs_lock_release(&lock);
    memset(text, 0, sizeof text);
    return saved;
}

bool rib_accounts_forget(const char *username, const char *game) {
    const char *root = folder();
    char key[KEY_SIZE + 1];
    char marker[PATH_SIZE];
    paths_t paths;
    fs_lock lock;
    bool others = false, forgotten;
    if (!root || !key_for(username, key) || !game_valid(game) || !paths_for(root, key, &paths)
        || !join(marker, paths.games, game) || !lock_folder(root, &lock))
        return false;
    forgotten = fs_remove(marker) == 0;
    if (forgotten && fs_is_directory(paths.games))
        forgotten = fs_list(paths.games, any_game, &others) >= 0;
    if (forgotten && !others)
        forgotten = erase_locked(&paths);
    fs_lock_release(&lock);
    return forgotten;
}

bool rib_accounts_drop_if(const char *username, const char *token) {
    const char *root = folder();
    char key[KEY_SIZE + 1];
    paths_t paths;
    rib_saved_account_t saved;
    fs_lock lock;
    bool dropped = true;
    if (!root || !key_for(username, key) || !token || !*token || !paths_for(root, key, &paths)
        || !lock_folder(root, &lock))
        return false;
    if (read_session(paths.session, &saved) && strcmp(saved.token, token) == 0)
        dropped = erase_locked(&paths);
    memset(&saved, 0, sizeof saved);
    fs_lock_release(&lock);
    return dropped;
}

bool rib_accounts_erase(const char *username) {
    const char *root = folder();
    char key[KEY_SIZE + 1];
    paths_t paths;
    fs_lock lock;
    bool erased;
    if (!root || !key_for(username, key) || !paths_for(root, key, &paths) || !lock_folder(root, &lock))
        return false;
    erased = erase_locked(&paths);
    fs_lock_release(&lock);
    return erased;
}

typedef struct listing {
    const char *root;
    rib_saved_account_t *out;
    size_t capacity;
    size_t count;
} listing_t;

static bool newer(const rib_saved_account_t *a, const rib_saved_account_t *b) {
    return a->used != b->used ? a->used > b->used : strcmp(a->username, b->username) < 0;
}

static int list_account(const char *name, void *context) {
    listing_t *listing = context;
    paths_t paths;
    rib_saved_account_t account;
    newest_t newest;
    size_t at;
    if (!key_valid(name) || !paths_for(listing->root, name, &paths) || !read_session(paths.session, &account))
        return 0;
    newest.games = paths.games;
    newest.used = -1;
    if (fs_is_directory(paths.games))
        fs_list(paths.games, newest_game, &newest);
    /* When a game last signed in, or the time of the session when no game
     * has recorded one. */
    account.used = newest.used >= 0 ? newest.used : fs_modified(paths.session);
    /* Kept in order as it fills, and only the newest `capacity` remain. */
    for (at = listing->count; at > 0 && newer(&account, &listing->out[at - 1]); --at)
        if (at < listing->capacity)
            listing->out[at] = listing->out[at - 1];
    if (at < listing->capacity) {
        listing->out[at] = account;
        if (listing->count < listing->capacity)
            listing->count++;
    }
    memset(&account, 0, sizeof account);
    return 0;
}

size_t rib_accounts_list(rib_saved_account_t *out, size_t capacity) {
    listing_t listing;
    listing.root = folder();
    listing.out = out;
    listing.capacity = capacity;
    listing.count = 0;
    if (!listing.root || !out || !capacity)
        return 0;
    fs_list(listing.root, list_account, &listing);
    return listing.count;
}
