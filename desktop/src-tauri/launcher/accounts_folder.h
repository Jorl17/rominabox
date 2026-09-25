#ifndef ROMINABOX_LAUNCHER_ACCOUNTS_FOLDER_H
#define ROMINABOX_LAUNCHER_ACCOUNTS_FOLDER_H

#include <stddef.h>

/* The folder that games share for QUICK SIGN IN. It is `name`, exactly as in
 * the export's launch plan, directly inside `app_data`, the platform's
 * per-user application data (Application Support in the real home on macOS,
 * %LOCALAPPDATA% on Windows). We make it private if it does not exist yet,
 * and look for nothing else.
 *
 * 0 with the path in `out`, or -1 when `name` is not a plain folder name, the
 * path does not fit, or the folder cannot be made. */
int rominabox_accounts_folder(const char *app_data, const char *name, char *out, size_t capacity);

#endif
