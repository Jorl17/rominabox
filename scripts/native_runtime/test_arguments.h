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
#include "windows/test_arguments.h"
#elif defined(__APPLE__) || defined(__unix__)
#include "posix/test_arguments.h"
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
