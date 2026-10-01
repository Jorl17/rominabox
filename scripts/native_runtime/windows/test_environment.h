#pragma once

#ifndef _WIN32
#error "windows/test_environment.h is how a test sets its environment on Windows; test_environment.h names each platform's"
#endif

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

static inline void test_setenv(const char *name, const char *value)
{
   wchar_t *wide_name = test_environment_wide(name);
   wchar_t *wide_value = test_environment_wide(value);
   if (_wputenv_s(wide_name, wide_value) != 0)
      abort();
   free(wide_name);
   free(wide_value);
}

static inline void test_unsetenv(const char *name)
{
   /* An empty value removes the variable. */
   wchar_t *wide_name = test_environment_wide(name);
   if (_wputenv_s(wide_name, L"") != 0)
      abort();
   free(wide_name);
}
