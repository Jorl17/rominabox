#pragma once

#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/test_folders.h is how a test makes its own folder on macOS and Linux; test_folders.h names each platform's"
#endif

#include <unistd.h>

/* `prefix` followed by six characters that no other folder there has. Stop
 * the program when we cannot make one. */
static inline void test_temporary_folder(char *out, size_t size, const char *prefix)
{
   snprintf(out, size, "/tmp/%sXXXXXX", prefix);
   if (!mkdtemp(out))
      abort();
}
