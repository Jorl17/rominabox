#pragma once

/* Set and clear, as UTF-8 on every platform and from C or C++, the
 * environment of a program under test. We read it in the player as in
 * vendor/retroarch/rominabox_environment.h (the wide environment on
 * Windows), so we set it the same way in a test, and a path under a folder
 * with a non-ASCII name then reaches the program intact. */

#include <stdlib.h>

#if defined(_WIN32)
#include "windows/test_environment.h"
#elif defined(__APPLE__) || defined(__unix__)
#include "posix/test_environment.h"
#else
#error "no way to set or clear the environment is declared for this platform"
#endif
