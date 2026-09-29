/* A Windows game made into one program contains the rest of the game packed
 * after the launcher (written in packaging/windows_pack.rs). Here we find the
 * pack, unpack it once into the game's own folder under the per-user
 * application data, and check that folder at every launch. */
#ifndef ROMINABOX_UNPACK_H
#define ROMINABOX_UNPACK_H

#include <stddef.h>
#include <wchar.h>

/* When `self`, this program, contains its game, make sure the game is
 * unpacked under `local_app_data`, with the unpacking dialog shown during
 * unpacking, return the folder (UTF-8) and the launcher in it, and return 1.
 * We show the dialog only when `shown`, so a quiet launch has none. Return 0
 * when this program contains no game, because it is part of a game laid out
 * as a folder, or the launcher that we already unpacked. */
int unpack_game(const wchar_t *self, const char *local_app_data, int shown, char *folder, size_t folder_cap,
                wchar_t *program, size_t program_cap);

#endif
