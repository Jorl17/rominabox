/* The parts of the Windows file layer (portable_fs.c) that take Windows'
 * own wide paths, for the Windows launcher. */
#ifndef ROMINABOX_PORTABLE_FS_WINDOWS_H
#define ROMINABOX_PORTABLE_FS_WINDOWS_H

#include <windows.h>

/* Move `source` to `target` with MoveFileExW and `flags`, replacing a file
 * that another process has open when `flags` has MOVEFILE_REPLACE_EXISTING.
 * A virus scanner keeps a file that it reads open for a moment, and until it
 * closes the file, Windows refuses to move the file or a folder that holds
 * it. So while Windows refuses for that reason, we try again, for at most a
 * few seconds. Return FALSE, with the last error set, when the move fails. */
BOOL fs_move_when_free(const wchar_t *source, const wchar_t *target, DWORD flags);

#endif
