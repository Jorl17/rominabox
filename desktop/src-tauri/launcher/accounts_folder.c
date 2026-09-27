#include "accounts_folder.h"

#include "portable_fs.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>

int rominabox_accounts_folder(const char *app_data, const char *name, char *out, size_t capacity) {
    /* We look only at the accounts folder. Inside the sandbox only that
     * folder can be opened, so even a query about its parent can fail. */
    if (!app_data || !app_data[0] || !name || !name[0] || name[0] == '.'
        || strpbrk(name, "/\\:\t\n")) {
        errno = EINVAL;
        return -1;
    }
    if (fs_join(out, capacity, app_data, name) != 0)
        return -1;
    if (fs_make_private_directory(out) != 0 || !fs_is_directory(out))
        return -1;
    return 0;
}
