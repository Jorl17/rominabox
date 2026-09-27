#include "portable_fs.h"

#include <errno.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32

#include <windows.h>
#include <fcntl.h>
#include <io.h>
#include <stdint.h>

/* We open every file so that another process can read, write, replace or
 * remove it while it is open, as on POSIX. Windows' own fopen does not allow
 * the last two, so a player could not sign out in one game while another game
 * reads the accounts folder. */
#define SHARE_ALL (FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
/* The path separator on Windows, where `/` is also accepted. */
#define FS_SEPARATOR '\\'

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

int fs_is_absolute(const char *path) {
    const int letter = (path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z');
    return (letter && path[1] == ':' && (path[2] == '\\' || path[2] == '/'))
        || ((path[0] == '\\' || path[0] == '/') && (path[1] == '\\' || path[1] == '/'));
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

/* Rename with POSIX semantics, replacing a file that another process has
 * open (Windows 10 1809 and later, on NTFS). 0 when that is not available
 * here, so that the caller can fall back. */
static BOOL replace_posix(const wchar_t *source, const wchar_t *target) {
    const size_t length = wcslen(target) * sizeof(wchar_t);
    FILE_RENAME_INFO *info = malloc(sizeof *info + length);
    HANDLE file;
    BOOL renamed = FALSE;
    if (!info)
        return FALSE;
    file = CreateFileW(source, DELETE | SYNCHRONIZE, SHARE_ALL, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (file != INVALID_HANDLE_VALUE) {
        memset(info, 0, sizeof *info);
        info->Flags = FILE_RENAME_FLAG_REPLACE_IF_EXISTS | FILE_RENAME_FLAG_POSIX_SEMANTICS;
        info->FileNameLength = (DWORD)length;
        memcpy(info->FileName, target, length + sizeof(wchar_t));
        renamed = SetFileInformationByHandle(file, FileRenameInfoEx, info, (DWORD)(sizeof *info + length));
        CloseHandle(file);
    }
    free(info);
    return renamed;
}

int fs_replace(const char *from, const char *to) {
    wchar_t *source = wide(from);
    wchar_t *target = source ? wide(to) : NULL;
    BOOL moved = FALSE;
    if (source && target) {
        moved = replace_posix(source, target)
            || MoveFileExW(source, target, MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH);
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

/* "r" or "w", with "b" for bytes: the modes this layer's callers use. */
FILE *fs_open(const char *path, const char *mode) {
    const int reading = mode[0] == 'r';
    int flags = (reading ? _O_RDONLY : _O_WRONLY) | (strchr(mode, 'b') ? _O_BINARY : _O_TEXT);
    wchar_t *name;
    HANDLE file;
    int descriptor;
    FILE *stream;
    if ((mode[0] != 'r' && mode[0] != 'w') || strchr(mode, '+')) {
        errno = EINVAL;
        return NULL;
    }
    name = wide(path);
    if (!name)
        return NULL;
    file = CreateFileW(name, reading ? GENERIC_READ : GENERIC_WRITE, SHARE_ALL, NULL,
                       reading ? OPEN_EXISTING : CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    free(name);
    if (file == INVALID_HANDLE_VALUE) {
        set_errno_from_windows();
        return NULL;
    }
    descriptor = _open_osfhandle((intptr_t)file, flags);
    if (descriptor < 0) {
        CloseHandle(file);
        return NULL;
    }
    stream = _fdopen(descriptor, mode);
    if (!stream)
        _close(descriptor);
    return stream;
}

int fs_make_private_directory(const char *path) {
    return fs_make_directory(path);
}

int fs_remove_directory(const char *path) {
    wchar_t *name = wide(path);
    BOOL removed;
    if (!name)
        return -1;
    removed = RemoveDirectoryW(name);
    free(name);
    if (!removed) {
        DWORD error = GetLastError();
        if (error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND)
            return 0;
        set_errno_from_windows();
        return -1;
    }
    return 0;
}

long long fs_modified(const char *path) {
    WIN32_FILE_ATTRIBUTE_DATA data;
    wchar_t *name = wide(path);
    ULARGE_INTEGER time;
    BOOL found;
    if (!name)
        return -1;
    found = GetFileAttributesExW(name, GetFileExInfoStandard, &data);
    free(name);
    if (!found)
        return -1;
    time.LowPart = data.ftLastWriteTime.dwLowDateTime;
    time.HighPart = data.ftLastWriteTime.dwHighDateTime;
    /* 100 ns steps since 1601, as seconds since 1970. */
    return (long long)(time.QuadPart / 10000000ULL) - 11644473600LL;
}

int fs_write_file(const char *path, const void *data, size_t size) {
    static LONG counter;
    size_t capacity = strlen(path) + 48;
    char *temporary;
    wchar_t *name;
    HANDLE file;
    DWORD wrote = 0;
    BOOL written;
    int attempt;
    if (size > MAXDWORD) {
        errno = EFBIG;
        return -1;
    }
    temporary = malloc(capacity);
    if (!temporary) {
        errno = ENOMEM;
        return -1;
    }
    snprintf(temporary, capacity, "%s.%lu-%ld.rominabox-new", path,
             (unsigned long)GetCurrentProcessId(), (long)InterlockedIncrement(&counter));
    name = wide(temporary);
    if (!name) {
        free(temporary);
        return -1;
    }
    file = CreateFileW(name, GENERIC_WRITE, 0, NULL, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, NULL);
    free(name);
    if (file == INVALID_HANDLE_VALUE) {
        set_errno_from_windows();
        free(temporary);
        return -1;
    }
    written = WriteFile(file, data, (DWORD)size, &wrote, NULL) && wrote == size
        && FlushFileBuffers(file);
    if (!written)
        set_errno_from_windows();
    CloseHandle(file);
    /* A virus scanner can keep the old file open for a moment. */
    for (attempt = 0; written && attempt < 50; ++attempt) {
        if (fs_replace(temporary, path) == 0) {
            free(temporary);
            return 0;
        }
        Sleep(10);
    }
    fs_remove(temporary);
    free(temporary);
    return -1;
}

int fs_lock_acquire(const char *path, fs_lock *lock) {
    OVERLAPPED whole;
    wchar_t *name = wide(path);
    HANDLE file;
    if (!name)
        return -1;
    file = CreateFileW(name, GENERIC_READ | GENERIC_WRITE,
                       FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, NULL,
                       OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    free(name);
    if (file == INVALID_HANDLE_VALUE) {
        set_errno_from_windows();
        return -1;
    }
    memset(&whole, 0, sizeof whole);
    if (!LockFileEx(file, LOCKFILE_EXCLUSIVE_LOCK, 0, MAXDWORD, MAXDWORD, &whole)) {
        set_errno_from_windows();
        CloseHandle(file);
        return -1;
    }
    lock->handle = (long long)(intptr_t)file;
    return 0;
}

void fs_lock_release(fs_lock *lock) {
    OVERLAPPED whole;
    if (!lock->handle)
        return;
    memset(&whole, 0, sizeof whole);
    UnlockFileEx((HANDLE)(intptr_t)lock->handle, 0, MAXDWORD, MAXDWORD, &whole);
    CloseHandle((HANDLE)(intptr_t)lock->handle);
    lock->handle = 0;
}

#else

#include <dirent.h>
#include <fcntl.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>

/* The path separator on macOS and Linux, and the only one there, because
 * `\` is part of a name on those platforms. */
#define FS_SEPARATOR '/'

int fs_is_absolute(const char *path) {
    return path[0] == '/';
}

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

int fs_make_private_directory(const char *path) {
    if (mkdir(path, 0700) == 0)
        return 0;
    if (errno == EEXIST && fs_is_directory(path))
        return 0;
    return -1;
}

int fs_remove_directory(const char *path) {
    return rmdir(path) == 0 || errno == ENOENT ? 0 : -1;
}

long long fs_modified(const char *path) {
    struct stat info;
    return lstat(path, &info) == 0 ? (long long)info.st_mtime : -1;
}

int fs_write_file(const char *path, const void *data, size_t size) {
    size_t capacity = strlen(path) + 32;
    char *temporary = malloc(capacity);
    const char *bytes = data;
    size_t done = 0;
    int descriptor;
    if (!temporary) {
        errno = ENOMEM;
        return -1;
    }
    snprintf(temporary, capacity, "%s.XXXXXX", path);
    /* mkstemp creates it 0600 and never over an existing file. */
    descriptor = mkstemp(temporary);
    if (descriptor < 0) {
        free(temporary);
        return -1;
    }
    while (done < size) {
        ssize_t wrote = write(descriptor, bytes + done, size - done);
        if (wrote < 0 && errno == EINTR)
            continue;
        if (wrote <= 0)
            break;
        done += (size_t)wrote;
    }
    if (done == size && fsync(descriptor) == 0 && close(descriptor) == 0) {
        descriptor = -1;
        if (rename(temporary, path) == 0) {
            free(temporary);
            return 0;
        }
    }
    {
        int saved = errno;
        if (descriptor >= 0)
            close(descriptor);
        unlink(temporary);
        free(temporary);
        errno = saved;
    }
    return -1;
}

int fs_lock_acquire(const char *path, fs_lock *lock) {
    int descriptor = open(path, O_RDWR | O_CREAT | O_CLOEXEC | O_NOFOLLOW, 0600);
    if (descriptor < 0)
        return -1;
    while (flock(descriptor, LOCK_EX) != 0) {
        if (errno != EINTR) {
            int saved = errno;
            close(descriptor);
            errno = saved;
            return -1;
        }
    }
    /* One more than the descriptor, so that a zeroed lock is empty. */
    lock->handle = (long long)descriptor + 1;
    return 0;
}

void fs_lock_release(fs_lock *lock) {
    if (!lock->handle)
        return;
    flock((int)(lock->handle - 1), LOCK_UN);
    close((int)(lock->handle - 1));
    lock->handle = 0;
}

#endif

void fs_native_path(char *path) {
    for (; *path; path++)
        if (*path == '/')
            *path = FS_SEPARATOR;
}

int fs_join(char *out, size_t capacity, const char *left, const char *right) {
    const size_t length = strlen(left);
    const char separator[2] = {FS_SEPARATOR, '\0'};
    const int separate = length > 0 && left[length - 1] != '/' && left[length - 1] != FS_SEPARATOR;
    const int wrote = snprintf(out, capacity, "%s%s%s", left, separate ? separator : "", right);
    if (wrote < 0 || (size_t)wrote >= capacity) {
        errno = ENAMETOOLONG;
        return -1;
    }
    fs_native_path(out);
    return 0;
}
