#ifndef _WIN32
#error "windows/platform.c is the accounts store test on Windows; the runner names each platform's"
#endif

/* On Windows we seal the token of an account to the user, and its folders
 * inherit the access list of the per-user folder, so there are no modes to
 * check. With no fork, a second process is this program started again. */
#include "../platform.h"

#include "accounts.h"

#include <assert.h>
#include <direct.h>
#include <io.h>
#include <process.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void test_set_variable(const char *name, const char *value)
{
   /* An empty value removes the variable. */
   assert(_putenv_s(name, value ? value : "") == 0);
}

void test_make_folder(const char *path)
{
   assert(_mkdir(path) == 0);
}

void test_make_run_folder(char *pattern)
{
   assert(_mktemp_s(pattern, strlen(pattern) + 1) == 0);
   test_make_folder(pattern);
}

void test_check_own_folder(const char *path)
{
   (void)path;
}

/* The program again, with the index. It reads the folder from the
 * environment it inherits. */
intptr_t test_start_process(const char *self, int index)
{
   char argument[16];
   intptr_t process;
   snprintf(argument, sizeof argument, "%d", index);
   process = _spawnl(_P_NOWAIT, self, self, "--process", argument, NULL);
   assert(process != -1);
   return process;
}

int test_finish_process(intptr_t process)
{
   int status;
   assert(_cwait(&status, process, 0) == process);
   return status;
}

/* Sealed to this Windows user: the file does not contain the token. */
void test_check_token_kept(const char *session, const char *token)
{
   assert(!strstr(session, token));
}

void test_check_private_folder(const char *path)
{
   (void)path;
}

void test_check_private_file(const char *path)
{
   (void)path;
}

/* A folder named like a share, \\server\name, is as absolute as one under a
 * drive letter. In the launcher we make the accounts folder in a per-user
 * folder that may be on a share, and we must accept it in the store. \\?\ is
 * the local form of such a name. */
static void a_share_path_names_the_folder(void)
{
   char share[1400];
   size_t index;
   test_reset();
   snprintf(share, sizeof share, "\\\\?\\%s", test_folder);
   for (index = 0; share[index]; ++index)
      if (share[index] == '/')
         share[index] = '\\';
   test_set_variable("ROMINABOX_ACCOUNTS_DIR", share);
   assert(rib_accounts_available());
   test_set_variable("ROMINABOX_ACCOUNTS_DIR", test_folder);
}

void test_platform_cases(void)
{
   a_share_path_names_the_folder();
}
