#ifndef _WIN32
#error "windows/paths.c is how a launch plan's paths read on Windows; the launcher recipe names each platform's"
#endif

#include "../paths.h"

#include <string.h>

int rominabox_path_is_separator(char c) {
    return c == '/' || c == '\\';
}

size_t rominabox_path_root_length(const char *path) {
    if (((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z'))
        && path[1] == ':' && rominabox_path_is_separator(path[2]))
        return 3;
    if (rominabox_path_is_separator(path[0]) && rominabox_path_is_separator(path[1])) {
        size_t index = 2;
        int separators = 0;
        while (path[index] && separators < 2) {
            if (rominabox_path_is_separator(path[index]))
                separators++;
            index++;
        }
        return index;
    }
    return 0;
}

int rominabox_path_names_a_drive(const char *path) {
    return strchr(path, ':') != NULL;
}

int rominabox_path_plain_part(const char *part, size_t length) {
    if (length == 0)
        return 0;
    return !memchr(part, ':', length) && part[length - 1] != '.' && part[length - 1] != ' ';
}
