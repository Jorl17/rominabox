#include "portable_fs.h"

#include <errno.h>
#include <string.h>

/* The code for every platform. The calls for each platform are in its
 * folder, posix/ or windows/, where we also declare its separator. */

void fs_native_path(char *path) {
    for (; *path; path++)
        if (*path == '/')
            *path = fs_separator;
}

int fs_join(char *out, size_t capacity, const char *left, const char *right) {
    const size_t length = strlen(left);
    const char separator[2] = {fs_separator, '\0'};
    const int separate = length > 0 && left[length - 1] != '/' && left[length - 1] != fs_separator;
    const int wrote = snprintf(out, capacity, "%s%s%s", left, separate ? separator : "", right);
    if (wrote < 0 || (size_t)wrote >= capacity) {
        errno = ENAMETOOLONG;
        return -1;
    }
    fs_native_path(out);
    return 0;
}
