#ifndef ROMINABOX_LAUNCHER_PORTABLE_FS_H
#define ROMINABOX_LAUNCHER_PORTABLE_FS_H

#include <stdio.h>

/* The file operations for the launcher's shared modules, with UTF-8 paths on
 * every platform. On macOS and Linux we use the POSIX calls (posix/). On Windows
 * we use the wide Win32 calls (windows/), because the narrow ones use the ANSI
 * code page for paths, which cannot represent every user folder name, and the
 * narrow rename() cannot replace an existing file.
 *
 * Every function returns 0 on success, or -1 with errno set, except as noted in
 * its comment. We follow a symbolic link or reparse point only in fs_open. */

/* 1 when `path` points to the same place whatever the working directory is:
 * on Windows C:\ or C:/, or a share, \\server\name, and elsewhere /. */
int fs_is_absolute(const char *path);

/* `left` and `right` as one path, with this platform's separator between
 * them and throughout, so on Windows we change every `/` to `\`. Windows
 * accepts either, but some programs that receive a path from the launcher do
 * not. For example, in a core the folder of a playlist ends at its last `/`.
 * Fail with ENAMETOOLONG when it does not fit. */
int fs_join(char *out, size_t capacity, const char *left, const char *right);
/* `path` spelled with this platform's separator throughout, in place. */
void fs_native_path(char *path);
/* This platform's separator: `\` on Windows, `/` on macOS and Linux. */
extern const char fs_separator;

/* Call `visit` with each entry's name, skipping names that start with '.'.
 * Fail with ENOENT for a missing directory. When `visit` returns non-zero,
 * stop the listing and return that value. */
typedef int (*fs_visit)(const char *name, void *context);
int fs_list(const char *directory, fs_visit visit, void *context);
/* As fs_list, with the names that start with '.' too, but never "." or
 * "..". */
int fs_list_all(const char *directory, fs_visit visit, void *context);

/* 1 when the path is that kind of thing, 0 when it is not or is absent. */
int fs_is_directory(const char *path);
int fs_is_file(const char *path);
int fs_exists(const char *path);

/* Succeeds when the directory already exists. */
int fs_make_directory(const char *path);
/* Copy the file `from` to `to`, unless something is at `to` already, which
 * also counts as success. We follow no link in either path. */
int fs_copy_new(const char *from, const char *to);
/* Moves `from` onto `to`, replacing a file already there. */
int fs_replace(const char *from, const char *to);
/* Succeeds when there is nothing to remove. */
int fs_remove(const char *path);
/* `mode` is "r" or "w", with "b" for bytes. Another process may still
 * replace or remove the file while it is open, on Windows as on POSIX. */
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
