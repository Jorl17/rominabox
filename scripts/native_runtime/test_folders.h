#pragma once

/* A new, empty folder for one test in the system's temporary folder, from
 * C or C++, with a UTF-8 path. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include "windows/test_folders.h"
#elif defined(__APPLE__) || defined(__unix__)
#include "posix/test_folders.h"
#else
#error "no way to make a test's own folder is declared for this platform"
#endif
