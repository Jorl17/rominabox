#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/paths.c is how a launch plan's paths read on macOS and Linux; the launcher recipe names each platform's"
#endif

#include "../paths.h"

int rominabox_path_is_separator(char c) {
    return c == '/';
}

size_t rominabox_path_root_length(const char *path) {
    return path[0] == '/' ? 1 : 0;
}

int rominabox_path_names_a_drive(const char *path) {
    (void)path;
    return 0;
}

int rominabox_path_plain_part(const char *part, size_t length) {
    if (length == 0)
        return 0;
    return !(length == 1 && part[0] == '.') && !(length == 2 && part[0] == '.' && part[1] == '.');
}
