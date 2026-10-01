#ifndef ROMINABOX_ACCOUNTS_STORE_TEST_PLATFORM_H
#define ROMINABOX_ACCOUNTS_STORE_TEST_PLATFORM_H

/* What we do differently on each platform in the accounts store test, one
 * file per platform (posix/, windows/), chosen as for the launcher: how we
 * set a variable, make a folder and start a second process, how we keep an
 * account private, and the cases for only one platform. */

#include <stdint.h>

/* The folder the current case works in, and a fresh one for the next case. */
extern char test_folder[1100];
void test_reset(void);

/* Process `index` of the case with several games at once, a writer or the
 * reader, run in this process, which it ends. */
void test_run_process(int index);

/* `name` set to `value`, or removed when `value` is NULL. */
void test_set_variable(const char *name, const char *value);
/* A new folder at `path`, private to this user. */
void test_make_folder(const char *path);
/* A new folder named from `pattern`, whose last six characters are XXXXXX
 * and are replaced in place, under the absolute folder in `pattern`. */
void test_make_run_folder(char *pattern);
/* Stops the run unless `path` is a folder this run may remove. */
void test_check_own_folder(const char *path);

/* Process `index` of the case with several games at once, started from this
 * program, `self`; and its exit code, or -1 when it did not exit on its own. */
intptr_t test_start_process(const char *self, int index);
int test_finish_process(intptr_t process);

/* How we keep an account private: whether a session file, as written,
 * contains `token`, and that only this user may open a folder or a file of
 * the store. */
void test_check_token_kept(const char *session, const char *token);
void test_check_private_folder(const char *path);
void test_check_private_file(const char *path);

/* The cases only this platform has. */
void test_platform_cases(void);

#endif
