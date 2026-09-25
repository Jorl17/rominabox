#include "accounts_folder.h"

#include "portable_fs.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>

int rominabox_accounts_folder(const char *app_data, const char *name, char *out, size_t capacity) {
    int wrote;
    if (!app_data || !app_data[0] || !name || !name[0] || name[0] == '.'
        || strpbrk(name, "/\\:\t\n") || !fs_is_directory(app_data)) {
        errno = EINVAL;
        return -1;
    }
    wrote = snprintf(out, capacity, "%s/%s", app_data, name);
    if (wrote < 0 || (size_t)wrote >= capacity) {
        errno = ENAMETOOLONG;
        return -1;
    }
    if (fs_make_private_directory(out) != 0 || !fs_is_directory(out))
        return -1;
    return 0;
}
