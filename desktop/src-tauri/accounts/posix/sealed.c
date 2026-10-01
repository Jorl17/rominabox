#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/sealed.c keeps a token as it is on macOS and Linux; the player recipe names each platform's"
#endif

#include "../sealed.h"

#include <stdlib.h>
#include <string.h>

bool rib_seal(const char *token, char *out, size_t capacity) {
    size_t length = strlen(token);
    if (length >= capacity)
        return false;
    memcpy(out, token, length + 1);
    return true;
}

bool rib_unseal(const char *sealed, char *out, size_t capacity) {
    return rib_seal(sealed, out, capacity);
}
