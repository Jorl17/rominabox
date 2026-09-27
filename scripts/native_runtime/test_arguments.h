#pragma once

/* The arguments of a test program as UTF-8, the encoding of a path in the
 * menu's file layer and in std::filesystem. POSIX systems pass them that way.
 * Windows passes argv in the ANSI code page, which cannot represent every
 * non-ASCII folder name, so there we convert the wide command line instead.
 *
 *   Utf8Arguments utf8(argc, argv);
 *   argv = utf8.argv();
 */

#include <string>
#include <vector>

#if defined(_WIN32)
#include <windows.h>
#include <shellapi.h>
#endif

class Utf8Arguments
{
public:
   Utf8Arguments(int argc, char **argv)
   {
#if defined(_WIN32)
      (void)argc;
      (void)argv;
      int count = 0;
      wchar_t **wide = CommandLineToArgvW(GetCommandLineW(), &count);
      for (int i = 0; wide && i < count; i++)
      {
         const int length = (int)wcslen(wide[i]);
         const int size = WideCharToMultiByte(CP_UTF8, 0, wide[i], length, nullptr, 0, nullptr, nullptr);
         std::string argument(size > 0 ? size : 0, ' ');
         if (size > 0)
            WideCharToMultiByte(CP_UTF8, 0, wide[i], length, &argument[0], size, nullptr, nullptr);
         strings.push_back(argument);
      }
      LocalFree(wide);
#elif defined(__APPLE__) || defined(__unix__)
      strings.assign(argv, argv + argc);
#else
#error "no way to read a program's arguments as UTF-8 is declared for this platform"
#endif
      for (std::string& argument : strings)
         pointers.push_back(&argument[0]);
      pointers.push_back(nullptr);
   }

   Utf8Arguments(const Utf8Arguments&) = delete;
   Utf8Arguments& operator=(const Utf8Arguments&) = delete;

   char **argv() { return pointers.data(); }
   int argc() const { return (int)strings.size(); }

private:
   std::vector<std::string> strings;
   std::vector<char*> pointers;
};
