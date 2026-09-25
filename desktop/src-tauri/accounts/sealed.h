/* A token in the form we store in the accounts folder. On Windows we seal it
 * to this user with DPAPI, so a copied file is useless elsewhere. On macOS
 * and Linux it stays unchanged, and the folder's 0700 is the protection. On
 * every platform, this user's own programs can still read the token. */
#ifndef ROMINABOX_ACCOUNTS_SEALED_H
#define ROMINABOX_ACCOUNTS_SEALED_H

#include <stdbool.h>
#include <stddef.h>

/* Both return failure when the result does not fit, or when the text was
 * not sealed for this user. */
bool rib_seal(const char *token, char *out, size_t capacity);
bool rib_unseal(const char *sealed, char *out, size_t capacity);

#endif
