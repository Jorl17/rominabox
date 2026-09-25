#include "portable_fs.h"

#include <errno.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32

#include <windows.h>

static void set_errno_from_windows(void) {
    switch (GetLastError()) {
    case ERROR_FILE_NOT_FOUND:
    case ERROR_PATH_NOT_FOUND:
    case ERROR_INVALID_DRIVE:
        errno = ENOENT;
        break;
    case ERROR_ACCESS_DENIED:
    case ERROR_SHARING_VIOLATION:
    case ERROR_LOCK_VIOLATION:
        errno = EACCES;
        break;
    case ERROR_ALREADY_EXISTS:
    case ERROR_FILE_EXISTS:
        errno = EEXIST;
        break;
    case ERROR_NOT_ENOUGH_MEMORY:
    case ERROR_OUTOFMEMORY:
        errno = ENOMEM;
        break;
    case ERROR_FILENAME_EXCED_RANGE:
        errno = ENAMETOOLONG;
        break;
    default:
        errno = EIO;
        break;
    }
}

/* A UTF-8 path as UTF-16, or NULL with errno set. The caller frees it. */
static wchar_t *wide(const char *path) {
    int length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    wchar_t *result;
    if (length <= 0) {
        errno = EINVAL;
        return NULL;
    }
    result = malloc((size_t)length * sizeof *result);
    if (!result) {
        errno = ENOMEM;
        return NULL;
    }
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, result, length);
    return result;
}

static char *narrow(const wchar_t *name) {
    int length = WideCharToMultiByte(CP_UTF8, 0, name, -1, NULL, 0, NULL, NULL);
    char *result;
    if (length <= 0) {
        errno = EINVAL;
        return NULL;
    }
    result = malloc((size_t)length);
    if (!result) {
        errno = ENOMEM;
        return NULL;
    }
    WideCharToMultiByte(CP_UTF8, 0, name, -1, result, length, NULL, NULL);
    return result;
}

/* The attributes, or INVALID_FILE_ATTRIBUTES when absent or unreadable. */
static DWORD attributes(const char *path) {
    wchar_t *name = wide(path);
    DWORD found;
    if (!name)
        return INVALID_FILE_ATTRIBUTES;
    found = GetFileAttributesW(name);
    free(name);
    return found;
}

int fs_list(const char *directory, fs_visit visit, void *context) {
    size_t length = strlen(directory);
    char *pattern = malloc(length + 3);
    wchar_t *wide_pattern;
    WIN32_FIND_DATAW entry;
    HANDLE search;
    int result = 0;
    if (!pattern) {
        errno = ENOMEM;
        return -1;
    }
    memcpy(pattern, directory, length);
    memcpy(pattern + length, "/*", 3);
    wide_pattern = wide(pattern);
    free(pattern);
    if (!wide_pattern)
        return -1;
    search = FindFirstFileW(wide_pattern, &entry);
    free(wide_pattern);
    if (search == INVALID_HANDLE_VALUE) {
        set_errno_from_windows();
        return -1;
    }
    do {
        char *name;
        if (entry.cFileName[0] == L'.')
            continue;
        name = narrow(entry.cFileName);
        if (!name) {
            result = -1;
            break;
        }
        result = visit(name, context);
        free(name);
    } while (result == 0 && FindNextFileW(search, &entry));
    if (result == 0 && GetLastError() != ERROR_NO_MORE_FILES) {
        set_errno_from_windows();
        result = -1;
    }
    FindClose(search);
    return result;
}

int fs_is_directory(const char *path) {
    DWORD found = attributes(path);
    return found != INVALID_FILE_ATTRIBUTES && (found & FILE_ATTRIBUTE_DIRECTORY)
        && !(found & FILE_ATTRIBUTE_REPARSE_POINT);
}

int fs_is_file(const char *path) {
    DWORD found = attributes(path);
    return found != INVALID_FILE_ATTRIBUTES && !(found & FILE_ATTRIBUTE_DIRECTORY)
        && !(found & FILE_ATTRIBUTE_REPARSE_POINT);
}

int fs_exists(const char *path) {
    return attributes(path) != INVALID_FILE_ATTRIBUTES;
}

int fs_make_directory(const char *path) {
    wchar_t *name = wide(path);
    BOOL made;
    if (!name)
        return -1;
    made = CreateDirectoryW(name, NULL);
    if (!made && GetLastError() == ERROR_ALREADY_EXISTS && fs_is_directory(path)) {
        free(name);
        return 0;
    }
    if (!made)
        set_errno_from_windows();
    free(name);
    return made ? 0 : -1;
}

int fs_replace(const char *from, const char *to) {
    wchar_t *source = wide(from);
    wchar_t *target = source ? wide(to) : NULL;
    BOOL moved = FALSE;
    if (source && target) {
        moved = MoveFileExW(source, target, MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH);
        if (!moved)
            set_errno_from_windows();
    }
    free(source);
    free(target);
    return moved ? 0 : -1;
}

int fs_remove(const char *path) {
    wchar_t *name = wide(path);
    BOOL removed;
    if (!name)
        return -1;
    removed = DeleteFileW(name);
    if (!removed) {
        DWORD error = GetLastError();
        free(name);
        if (error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND)
            return 0;
        set_errno_from_windows();
        return -1;
    }
    free(name);
    return 0;
}

FILE *fs_open(const char *path, const char *mode) {
    wchar_t *name = wide(path);
    wchar_t *wide_mode = name ? wide(mode) : NULL;
    FILE *stream = NULL;
    if (name && wide_mode)
        stream = _wfopen(name, wide_mode);
    free(name);
    free(wide_mode);
    return stream;
}

#else

#include <dirent.h>
#include <sys/stat.h>
#include <unistd.h>

int fs_list(const char *directory, fs_visit visit, void *context) {
    DIR *listing = opendir(directory);
    struct dirent *entry;
    int result = 0;
    if (!listing)
        return -1;
    while (result == 0) {
        errno = 0;
        entry = readdir(listing);
        if (!entry) {
            if (errno != 0)
                result = -1;
            break;
        }
        if (entry->d_name[0] == '.')
            continue;
        result = visit(entry->d_name, context);
    }
    {
        int saved = errno;
        closedir(listing);
        errno = saved;
    }
    return result;
}

int fs_is_directory(const char *path) {
    struct stat info;
    return lstat(path, &info) == 0 && S_ISDIR(info.st_mode);
}

int fs_is_file(const char *path) {
    struct stat info;
    return lstat(path, &info) == 0 && S_ISREG(info.st_mode);
}

int fs_exists(const char *path) {
    struct stat info;
    return lstat(path, &info) == 0;
}

int fs_make_directory(const char *path) {
    if (mkdir(path, 0755) == 0)
        return 0;
    if (errno == EEXIST && fs_is_directory(path))
        return 0;
    return -1;
}

int fs_replace(const char *from, const char *to) {
    return rename(from, to);
}

int fs_remove(const char *path) {
    return unlink(path) == 0 || errno == ENOENT ? 0 : -1;
}

FILE *fs_open(const char *path, const char *mode) {
    return fopen(path, mode);
}

#endif
