#pragma once

/* Set and clear, as UTF-8 on every platform, the environment that we read in
 * a menu under test. We read it in the menu as in the player
 * (vendor/retroarch/rominabox_environment.h: the wide environment on
 * Windows), so we set it the same way in a test, and a path under a folder
 * with a non-ASCII name then reaches the menu intact. */

#include <cstdlib>
#include <string>

#if defined(_WIN32)
#include <windows.h>

namespace rib_test_environment {
inline std::wstring wide(const char *utf8)
{
   const int size = MultiByteToWideChar(CP_UTF8, 0, utf8, -1, nullptr, 0);
   std::wstring text(size > 0 ? size - 1 : 0, L' ');
   if (size > 1)
      MultiByteToWideChar(CP_UTF8, 0, utf8, -1, &text[0], size);
   return text;
}
}
#endif

inline void test_setenv(const char *name, const char *value)
{
#if defined(_WIN32)
   _wputenv_s(rib_test_environment::wide(name).c_str(), rib_test_environment::wide(value).c_str());
#elif defined(__APPLE__) || defined(__unix__)
   setenv(name, value, 1);
#else
#error "no way to set the environment is declared for this platform"
#endif
}

inline void test_unsetenv(const char *name)
{
#if defined(_WIN32)
   /* An empty value removes the variable. */
   _wputenv_s(rib_test_environment::wide(name).c_str(), L"");
#elif defined(__APPLE__) || defined(__unix__)
   unsetenv(name);
#else
#error "no way to clear the environment is declared for this platform"
#endif
}
