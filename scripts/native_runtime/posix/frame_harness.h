#pragma once

#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/frame_harness.h is how the frame harness loads a core on macOS and Linux; frame_harness.c names each platform's"
#endif

#include <dlfcn.h>

typedef void *core_library;

static core_library open_core(const char *path)
{
    void *library = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!library) fprintf(stderr, "%s\n", dlerror());
    return library;
}

static void *core_symbol(core_library library, const char *name) { return dlsym(library, name); }

static void close_core(core_library library) { dlclose(library); }

static FILE *open_file(const char *path, const char *mode) { return fopen(path, mode); }
