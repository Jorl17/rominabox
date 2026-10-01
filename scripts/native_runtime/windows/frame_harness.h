#pragma once

#ifndef _WIN32
#error "windows/frame_harness.h is how the frame harness loads a core on Windows; frame_harness.c names each platform's"
#endif

#include <windows.h>

typedef HMODULE core_library;

static core_library open_core(const char *path)
{
    wchar_t *wide = test_environment_wide(path);
    HMODULE library = LoadLibraryW(wide);
    free(wide);
    if (!library) fprintf(stderr, "cannot load %s: error %lu\n", path, GetLastError());
    return library;
}

static void *core_symbol(core_library library, const char *name)
{
    return (void *)GetProcAddress(library, name);
}

static void close_core(core_library library) { FreeLibrary(library); }

static FILE *open_file(const char *path, const char *mode)
{
    wchar_t *wide_path = test_environment_wide(path), *wide_mode = test_environment_wide(mode);
    FILE *file = _wfopen(wide_path, wide_mode);
    free(wide_path);
    free(wide_mode);
    return file;
}
