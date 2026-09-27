#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/portable_fs.c is the file layer of macOS and Linux; the launcher recipe names each platform's"
#endif

#include "../portable_fs.h"

#include <errno.h>
#include <stdlib.h>
#include <string.h>

#include <dirent.h>
#include <fcntl.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>

/* The path separator on macOS and Linux, and the only one there, because
 * `\` is part of a name on those platforms. */
const char fs_separator = '/';

int fs_is_absolute(const char *path) {
    return path[0] == '/';
}

/* "." and "..", which are in every listing. With `hidden` we also skip any
 * name that starts with '.'. */
static int skipped(const char *name, int hidden) {
    if (name[0] != '.')
        return 0;
    return hidden || name[1] == '\0' || (name[1] == '.' && name[2] == '\0');
}

static int list_names(const char *directory, int hidden, fs_visit visit, void *context) {
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
        if (skipped(entry->d_name, hidden))
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

int fs_list(const char *directory, fs_visit visit, void *context) {
    return list_names(directory, 1, visit, context);
}

int fs_list_all(const char *directory, fs_visit visit, void *context) {
    return list_names(directory, 0, visit, context);
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

int fs_copy_new(const char *from, const char *to) {
    char buffer[8192];
    int in;
    int out;
    ssize_t count;
    struct stat info;
    if (lstat(to, &info) == 0)
        return 0;
    in = open(from, O_RDONLY | O_NOFOLLOW);
    if (in < 0)
        return -1;
    out = open(to, O_WRONLY | O_CREAT | O_EXCL, 0644);
    if (out < 0) {
        close(in);
        if (errno == EEXIST)
            return 0;
        return -1;
    }
    while ((count = read(in, buffer, sizeof buffer)) > 0) {
        char *cursor = buffer;
        while (count > 0) {
            ssize_t wrote = write(out, cursor, (size_t)count);
            if (wrote < 0) {
                close(in);
                close(out);
                unlink(to);
                return -1;
            }
            cursor += wrote;
            count -= wrote;
        }
    }
    close(in);
    if (close(out) != 0) {
        unlink(to);
        return -1;
    }
    return count < 0 ? -1 : 0;
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
