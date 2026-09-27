#pragma once

/* The arguments of a test program as UTF-8, the encoding of a path in the
 * menu's file layer, std::filesystem and libretro cores, from C or C++.
 * POSIX systems pass them that way. Windows passes argv in the ANSI code
 * page, which cannot represent every non-ASCII folder name, so there we
 * convert the wide command line instead.
 *
 *   argv = test_utf8_argv(&argc, argv);      (C)
 *   Utf8Arguments utf8(argc, argv);          (C++)
 *   argv = utf8.argv();
 */

#include <stdlib.h>

#if defined(_WIN32)
#include <windows.h>
#include <shellapi.h>

/* The arguments as UTF-8, allocated once for the life of the program. */
static inline char **test_utf8_argv(int *argc, char **argv)
{
   int count = 0;
   wchar_t **wide = CommandLineToArgvW(GetCommandLineW(), &count);
   char **utf8 = (char**)calloc((size_t)(count > 0 ? count : 0) + 1, sizeof *utf8);
   (void)argv;
   if (!wide || !utf8)
      abort();
   for (int i = 0; i < count; i++)
   {
      const int size = WideCharToMultiByte(CP_UTF8, 0, wide[i], -1, NULL, 0, NULL, NULL);
      utf8[i] = (char*)malloc(size > 0 ? (size_t)size : 1);
      if (!utf8[i])
         abort();
      utf8[i][0] = '\0';
      if (size > 0)
         WideCharToMultiByte(CP_UTF8, 0, wide[i], -1, utf8[i], size, NULL, NULL);
   }
   LocalFree(wide);
   *argc = count;
   return utf8;
}
#elif defined(__APPLE__) || defined(__unix__)
static inline char **test_utf8_argv(int *argc, char **argv)
{
   (void)argc;
   return argv;
}
#else
#error "no way to read a program's arguments as UTF-8 is declared for this platform"
#endif

#ifdef __cplusplus
class Utf8Arguments
{
public:
   Utf8Arguments(int argc, char **argv) : count(argc), values(test_utf8_argv(&count, argv)) {}

   Utf8Arguments(const Utf8Arguments&) = delete;
   Utf8Arguments& operator=(const Utf8Arguments&) = delete;

   char **argv() { return values; }
   int argc() const { return count; }

private:
   int count;
   char **values;
};
#endif
