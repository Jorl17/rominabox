#pragma once

/* Start a program, write `input` to its stdin and wait. Return its exit code,
 * or -1 when it could not start or did not exit normally. It shares stdout and
 * stderr with the caller. No shell is involved, so we pass a path unchanged.
 * Only starting and waiting differ between platforms. */

#include <string>
#include <vector>

#if defined(_WIN32)
#include "windows/test_process.h"
#elif defined(__APPLE__) || defined(__unix__)
#include "posix/test_process.h"
#else
#error "no way to start a process is declared for this platform"
#endif
