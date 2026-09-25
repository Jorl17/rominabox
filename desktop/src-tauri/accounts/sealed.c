#include "sealed.h"

#include <stdlib.h>
#include <string.h>

#ifdef _WIN32

#include <windows.h>
#include <wincrypt.h>

static const char digits[] = "0123456789abcdef";

static int digit(char c) {
    return c >= '0' && c <= '9' ? c - '0' : c >= 'a' && c <= 'f' ? c - 'a' + 10 : -1;
}

bool rib_seal(const char *token, char *out, size_t capacity) {
    DATA_BLOB plain, sealed;
    DWORD index;
    bool fits;
    plain.pbData = (BYTE *)token;
    plain.cbData = (DWORD)strlen(token);
    if (!CryptProtectData(&plain, NULL, NULL, NULL, NULL, CRYPTPROTECT_UI_FORBIDDEN, &sealed))
        return false;
    fits = (size_t)sealed.cbData * 2 < capacity;
    for (index = 0; fits && index < sealed.cbData; ++index) {
        out[index * 2] = digits[sealed.pbData[index] >> 4];
        out[index * 2 + 1] = digits[sealed.pbData[index] & 15];
    }
    if (fits)
        out[sealed.cbData * 2] = '\0';
    LocalFree(sealed.pbData);
    return fits;
}

bool rib_unseal(const char *sealed, char *out, size_t capacity) {
    size_t length = strlen(sealed), index;
    DATA_BLOB blob, plain;
    bool fits;
    if (!length || length % 2 || length / 2 > MAXDWORD)
        return false;
    blob.cbData = (DWORD)(length / 2);
    blob.pbData = malloc(blob.cbData);
    if (!blob.pbData)
        return false;
    for (index = 0; index < blob.cbData; ++index) {
        int high = digit(sealed[index * 2]), low = digit(sealed[index * 2 + 1]);
        if (high < 0 || low < 0) {
            free(blob.pbData);
            return false;
        }
        blob.pbData[index] = (BYTE)(high << 4 | low);
    }
    fits = CryptUnprotectData(&blob, NULL, NULL, NULL, NULL, CRYPTPROTECT_UI_FORBIDDEN, &plain) != 0;
    free(blob.pbData);
    if (!fits)
        return false;
    fits = plain.cbData < capacity && !memchr(plain.pbData, '\0', plain.cbData);
    if (fits) {
        memcpy(out, plain.pbData, plain.cbData);
        out[plain.cbData] = '\0';
    }
    SecureZeroMemory(plain.pbData, plain.cbData);
    LocalFree(plain.pbData);
    return fits;
}

#else

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

#endif
