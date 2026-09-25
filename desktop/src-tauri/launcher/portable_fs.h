#ifndef ROMINABOX_LAUNCHER_PORTABLE_FS_H
#define ROMINABOX_LAUNCHER_PORTABLE_FS_H

#include <stdio.h>

/* The file operations for the launcher's shared modules, with UTF-8 paths on
 * every platform. On macOS and Linux we use the POSIX calls. On Windows we use
 * the wide Win32 calls, because the narrow ones use the ANSI code page for
 * paths, which cannot represent every user folder name, and the narrow rename()
 * cannot replace an existing file.
 *
 * Every function returns 0 on success, or -1 with errno set, except as noted in
 * its comment. We follow a symbolic link or reparse point only in fs_open. */

/* Call `visit` with each entry's name, skipping names that start with '.'.
 * Fail with ENOENT for a missing directory. When `visit` returns non-zero,
 * stop the listing and return that value. */
typedef int (*fs_visit)(const char *name, void *context);
int fs_list(const char *directory, fs_visit visit, void *context);

/* 1 when the path is that kind of thing, 0 when it is not or is absent. */
int fs_is_directory(const char *path);
int fs_is_file(const char *path);
int fs_exists(const char *path);

/* Succeeds when the directory already exists. */
int fs_make_directory(const char *path);
/* Moves `from` onto `to`, replacing a file already there. */
int fs_replace(const char *from, const char *to);
/* Succeeds when there is nothing to remove. */
int fs_remove(const char *path);
FILE *fs_open(const char *path, const char *mode);

/* Only this user may enter it: 0700, or on Windows the access list a folder
 * under %LOCALAPPDATA% inherits. Succeeds when it already exists. */
int fs_make_private_directory(const char *path);
/* An empty directory. Succeeds when there is nothing to remove. */
int fs_remove_directory(const char *path);
/* Seconds since 1970 of the last write, or -1 when absent. */
long long fs_modified(const char *path);
/* `size` bytes as the whole of `path`, readable by this user only. We write
 * beside it and swap it in, so a reader finds the old file or the new one. */
int fs_write_file(const char *path, const void *data, size_t size);

/* An exclusive lock on the file at `path`, created if needed. Wait until no
 * other process has the lock. */
typedef struct fs_lock {
    long long handle;
} fs_lock;
int fs_lock_acquire(const char *path, fs_lock *lock);
void fs_lock_release(fs_lock *lock);

#endif
