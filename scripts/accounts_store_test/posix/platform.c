#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/platform.c is the accounts store test on macOS and Linux; the runner names each platform's"
#endif

/* On macOS and Linux an account is private through the modes of its folders,
 * and we store its token as given. A second process is this one forked. */
#include "../platform.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

static mode_t mode_of(const char *path)
{
   struct stat info;
   assert(stat(path, &info) == 0);
   return info.st_mode & 0777;
}

void test_set_variable(const char *name, const char *value)
{
   assert(value ? setenv(name, value, 1) == 0 : unsetenv(name) == 0);
}

void test_make_folder(const char *path)
{
   assert(mkdir(path, 0700) == 0);
}

void test_make_run_folder(char *pattern)
{
   assert(pattern[0] == '/' && mkdtemp(pattern));
}

/* A folder, not a link to one, and this user's. */
void test_check_own_folder(const char *path)
{
   struct stat info;
   assert(lstat(path, &info) == 0 && S_ISDIR(info.st_mode) && info.st_uid == getuid());
}

intptr_t test_start_process(const char *self, int index)
{
   pid_t child = fork();
   (void)self;
   assert(child >= 0);
   if (child == 0)
      test_run_process(index);
   return child;
}

int test_finish_process(intptr_t process)
{
   int status;
   assert(waitpid((pid_t)process, &status, 0) == (pid_t)process);
   return WIFEXITED(status) ? WEXITSTATUS(status) : -1;
}

/* The folder modes are the protection, and the token is a separate line
 * in the session file, as given. */
void test_check_token_kept(const char *session, const char *token)
{
   char line[256];
   snprintf(line, sizeof line, "\n%s\n", token);
   assert(strstr(session, line));
}

void test_check_private_folder(const char *path)
{
   assert(mode_of(path) == 0700);
}

void test_check_private_file(const char *path)
{
   assert(mode_of(path) == 0600);
}

void test_platform_cases(void)
{
}
