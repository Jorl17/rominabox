#pragma once

/* Set and clear, as UTF-8 on every platform and from C or C++, the
 * environment of a program under test. We read it in the player as in
 * vendor/retroarch/rominabox_environment.h (the wide environment on
 * Windows), so we set it the same way in a test, and a path under a folder
 * with a non-ASCII name then reaches the program intact. */

#include <stdlib.h>

#if defined(_WIN32)
#include <windows.h>

/* `utf8` as UTF-16, for the caller to free. */
static inline wchar_t *test_environment_wide(const char *utf8)
{
   const int size = MultiByteToWideChar(CP_UTF8, 0, utf8, -1, NULL, 0);
   wchar_t *text = (wchar_t*)malloc((size > 0 ? (size_t)size : 1) * sizeof *text);
   if (!text)
      abort();
   text[0] = L'\0';
   if (size > 0)
      MultiByteToWideChar(CP_UTF8, 0, utf8, -1, text, size);
   return text;
}
#endif

static inline void test_setenv(const char *name, const char *value)
{
#if defined(_WIN32)
   wchar_t *wide_name = test_environment_wide(name);
   wchar_t *wide_value = test_environment_wide(value);
   if (_wputenv_s(wide_name, wide_value) != 0)
      abort();
   free(wide_name);
   free(wide_value);
#elif defined(__APPLE__) || defined(__unix__)
   if (setenv(name, value, 1) != 0)
      abort();
#else
#error "no way to set the environment is declared for this platform"
#endif
}

static inline void test_unsetenv(const char *name)
{
#if defined(_WIN32)
   /* An empty value removes the variable. */
   wchar_t *wide_name = test_environment_wide(name);
   if (_wputenv_s(wide_name, L"") != 0)
      abort();
   free(wide_name);
#elif defined(__APPLE__) || defined(__unix__)
   if (unsetenv(name) != 0)
      abort();
#else
#error "no way to clear the environment is declared for this platform"
#endif
}
