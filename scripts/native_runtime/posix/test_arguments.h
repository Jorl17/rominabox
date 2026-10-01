#pragma once

#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/test_arguments.h is how a test reads its arguments on macOS and Linux; test_arguments.h names each platform's"
#endif

static inline char **test_utf8_argv(int *argc, char **argv)
{
   (void)argc;
   return argv;
}
