#pragma once

/* A new, empty folder for one test in the system's temporary folder, from
 * C or C++, with a UTF-8 path. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include <windows.h>
#else
#include <unistd.h>
#endif

/* `prefix` followed by six characters that no other folder there has. Stop
 * the program when we cannot make one. */
static inline void test_temporary_folder(char *out, size_t size, const char *prefix)
{
#if defined(_WIN32)
   wchar_t base[MAX_PATH + 1];
   char folder[MAX_PATH * 3];
   const DWORD length = GetTempPathW(MAX_PATH + 1, base);
   int attempt;
   if (!length || length > MAX_PATH
         || !WideCharToMultiByte(CP_UTF8, 0, base, -1, folder, sizeof folder, NULL, NULL))
      abort();
   for (attempt = 0; attempt < 100; ++attempt)
   {
      wchar_t *wide;
      int wide_size;
      BOOL made;
      snprintf(out, size, "%s%s%06x", folder, prefix,
               (unsigned)((GetCurrentProcessId() * 2654435761u + GetTickCount() + attempt * 7919u) & 0xffffff));
      wide_size = MultiByteToWideChar(CP_UTF8, 0, out, -1, NULL, 0);
      wide = (wchar_t*)malloc((size_t)wide_size * sizeof *wide);
      if (!wide)
         abort();
      MultiByteToWideChar(CP_UTF8, 0, out, -1, wide, wide_size);
      made = CreateDirectoryW(wide, NULL);
      free(wide);
      if (made)
         return;
   }
   abort();
#else
   snprintf(out, size, "/tmp/%sXXXXXX", prefix);
   if (!mkdtemp(out))
      abort();
#endif
}
