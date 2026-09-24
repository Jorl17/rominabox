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

#endif
