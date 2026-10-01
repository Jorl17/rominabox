#ifndef _WIN32
#error "windows/main.cpp is rml-preview's entry on Windows; the player recipe names each platform's"
#endif

#include "../rml_preview.h"

#include <string>
#include <vector>

/* The arguments as UTF-8. On Windows, main receives them in the ANSI code
 * page, which cannot represent every folder name, so we use the wide entry. */
#include <windows.h>

int wmain(int argc, wchar_t **argv)
{
   std::vector<std::string> arguments;
   for (int index = 0; index < argc; index++)
   {
      const int size = WideCharToMultiByte(CP_UTF8, 0, argv[index], -1, nullptr, 0, nullptr, nullptr);
      std::string argument(size > 0 ? (size_t)size - 1 : 0, '\0');
      if (size > 1)
         WideCharToMultiByte(CP_UTF8, 0, argv[index], -1, &argument[0], size, nullptr, nullptr);
      arguments.push_back(argument);
   }
   return run(arguments);
}
