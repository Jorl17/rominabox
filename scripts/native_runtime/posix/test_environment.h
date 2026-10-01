#pragma once

#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/test_environment.h is how a test sets its environment on macOS and Linux; test_environment.h names each platform's"
#endif

static inline void test_setenv(const char *name, const char *value)
{
   if (setenv(name, value, 1) != 0)
      abort();
}

static inline void test_unsetenv(const char *name)
{
   if (unsetenv(name) != 0)
      abort();
}
