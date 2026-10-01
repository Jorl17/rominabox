#ifndef ROMINABOX_LAUNCHER_PATHS_H
#define ROMINABOX_LAUNCHER_PATHS_H

#include <stddef.h>

/* How we read a path in a launch plan on each platform: posix/paths.c on
 * macOS and Linux, windows/paths.c on Windows. The launcher recipe lists the
 * file for each platform. */

/* 1 when `c` separates a path's parts: `/` everywhere, and `\` on Windows
 * too. */
int rominabox_path_is_separator(char c);

/* The length of the root at the start of `path`, which we never create:
 * `/`, or on Windows a drive (C:\) or a share (\\server\share\). */
size_t rominabox_path_root_length(const char *path);

/* 1 on Windows when `path` starts with a drive, C:, which points outside the
 * folder it is joined to, as a root does. macOS and Linux have no drives. */
int rominabox_path_names_a_drive(const char *path);

/* `part`, `length` bytes of a path, is the name of a folder inside the one
 * before it. It is not empty, `.` or `..`, and on Windows it contains no
 * drive or stream separator (`:`) and does not end in `.` or a space, which
 * Windows drops. */
int rominabox_path_plain_part(const char *part, size_t length);

#endif
