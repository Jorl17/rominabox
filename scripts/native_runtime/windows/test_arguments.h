#pragma once

#ifndef _WIN32
#error "windows/test_arguments.h is how a test reads its arguments on Windows; test_arguments.h names each platform's"
#endif

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
