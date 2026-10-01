/* Unpacking a Windows game made into one program (unpack.h). The layout of
 * the pack is defined in packaging/windows_pack.rs. We write the folder only
 * from this program, outside the game's sandbox. The sandbox has read access
 * and no write access, so no change made inside lasts until the next launch.
 * We write everything into a new folder beside it and rename that into place
 * once complete, and no name in the pack may point outside that folder. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <bcrypt.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <zstd.h>

#include "../launch.h"
#include "unpack.h"
#include "unpack_dialog.h"

#define RIB_USER_FOLDER(name, path) static const char user_folder_##name[] = path;
#include "../launch_contract.inc"

#define TRAILER_SIZE 24
#define END_BYTES (64u * 1024u)
#define CHUNK (1u << 20)

typedef struct {
    char *path;
    uint64_t offset;
    uint64_t packed;
    uint64_t size;
    unsigned char whole[32];
    int read_whole;
    unsigned char ends[32];
} Entry;

typedef struct {
    unsigned char *index;
    char *runtime;
    char *program;
    unsigned char head_hash[32];
    uint64_t head_size;
    const unsigned char *logo;
    uint32_t logo_size;
    const unsigned char *font;
    uint32_t font_size;
    uint32_t count;
    Entry *entries;
} Pack;

typedef struct {
    const unsigned char *at;
    const unsigned char *end;
} Reader;

static void broken(void) {
    rominabox_launch_die("the game's program is damaged: download or export it again");
}

static const unsigned char *take(Reader *reader, size_t size) {
    const unsigned char *at = reader->at;
    if ((size_t)(reader->end - reader->at) < size)
        broken();
    reader->at += size;
    return at;
}

static uint64_t number(Reader *reader, size_t size) {
    const unsigned char *bytes = take(reader, size);
    uint64_t value = 0;
    while (size--)
        value = value << 8 | bytes[size];
    return value;
}

static char *text(Reader *reader, size_t length_size) {
    size_t length = (size_t)number(reader, length_size);
    const unsigned char *bytes = take(reader, length);
    char *copy = malloc(length + 1);
    if (!copy)
        rominabox_launch_die("out of memory");
    memcpy(copy, bytes, length);
    copy[length] = '\0';
    return copy;
}

static wchar_t *to_wide(const char *utf8) {
    int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, utf8, -1, NULL, 0);
    wchar_t *wide = size > 0 ? malloc((size_t)size * sizeof *wide) : NULL;
    if (!wide || !MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, utf8, -1, wide, size))
        broken();
    return wide;
}

/* A name that stays inside the folder: parts joined by "/", none empty, "."
 * or "..", and nothing that means a drive, a device path or a stream on Windows. */
static int stays_inside(const char *path) {
    const char *part = path;
    if (!*path)
        return 0;
    for (;;) {
        const char *end = strchr(part, '/');
        size_t length = end ? (size_t)(end - part) : strlen(part);
        if (length == 0 || (length == 1 && part[0] == '.') || (length == 2 && part[0] == '.' && part[1] == '.'))
            return 0;
        for (size_t index = 0; index < length; index++)
            if (part[index] == '\\' || part[index] == ':' || (unsigned char)part[index] < ' ')
                return 0;
        if (!end)
            return 1;
        part = end + 1;
    }
}

/* A folder of its own directly inside the runtimes folder. We unpack a game
 * only there, because when we unpack, we remove the folders beside it that
 * are named for the same game. */
static int in_runtimes(const char *runtime) {
    size_t length = strlen(user_folder_Runtimes);
    return stays_inside(runtime) && strncmp(runtime, user_folder_Runtimes, length) == 0 && runtime[length] == '/'
           && !strchr(runtime + length + 1, '/');
}

static void read_at(HANDLE file, uint64_t offset, void *buffer, DWORD size) {
    LARGE_INTEGER where;
    DWORD got = 0;
    where.QuadPart = (LONGLONG)offset;
    if (!SetFilePointerEx(file, where, NULL, FILE_BEGIN) || !ReadFile(file, buffer, size, &got, NULL) || got != size)
        broken();
}

/* The pack after this program, or 0 when there is none. */
static int find_pack(HANDLE file, Pack *pack) {
    LARGE_INTEGER size;
    unsigned char trailer[TRAILER_SIZE];
    Reader reader;
    uint64_t index_offset;
    uint64_t index_size;
    memset(pack, 0, sizeof *pack);
    if (!GetFileSizeEx(file, &size) || size.QuadPart < TRAILER_SIZE)
        return 0;
    read_at(file, (uint64_t)size.QuadPart - TRAILER_SIZE, trailer, TRAILER_SIZE);
    if (memcmp(trailer + 16, "RIBTAIL1", 8) != 0)
        return 0;
    reader.at = trailer;
    reader.end = trailer + 16;
    index_offset = number(&reader, 8);
    index_size = number(&reader, 8);
    if (index_offset + index_size + TRAILER_SIZE != (uint64_t)size.QuadPart || index_size > 64u << 20)
        broken();
    pack->index = malloc((size_t)index_size);
    if (!pack->index)
        rominabox_launch_die("out of memory");
    read_at(file, index_offset, pack->index, (DWORD)index_size);
    reader.at = pack->index;
    reader.end = pack->index + index_size;
    if (memcmp(take(&reader, 8), "RIBPACK1", 8) != 0)
        broken();
    pack->runtime = text(&reader, 2);
    pack->program = text(&reader, 2);
    memcpy(pack->head_hash, take(&reader, 32), 32);
    pack->logo_size = (uint32_t)number(&reader, 4);
    pack->logo = take(&reader, pack->logo_size);
    pack->font_size = (uint32_t)number(&reader, 4);
    pack->font = take(&reader, pack->font_size);
    pack->count = (uint32_t)number(&reader, 4);
    pack->entries = calloc(pack->count ? pack->count : 1, sizeof *pack->entries);
    if (!pack->entries)
        rominabox_launch_die("out of memory");
    pack->head_size = index_offset;
    for (uint32_t index = 0; index < pack->count; index++) {
        Entry *entry = &pack->entries[index];
        entry->path = text(&reader, 2);
        entry->offset = number(&reader, 8);
        entry->packed = number(&reader, 8);
        entry->size = number(&reader, 8);
        memcpy(entry->whole, take(&reader, 32), 32);
        entry->read_whole = *take(&reader, 1) != 0;
        memcpy(entry->ends, take(&reader, 32), 32);
        if (!stays_inside(entry->path) || entry->offset + entry->packed > index_offset)
            broken();
        if (entry->offset < pack->head_size)
            pack->head_size = entry->offset;
    }
    if (!in_runtimes(pack->runtime) || !stays_inside(pack->program) || strchr(pack->program, '/'))
        broken();
    return 1;
}

/* SHA-256, by Windows. */
typedef struct {
    BCRYPT_ALG_HANDLE algorithm;
    BCRYPT_HASH_HANDLE hash;
} Sha;

static void sha_start(Sha *sha) {
    if (BCryptOpenAlgorithmProvider(&sha->algorithm, BCRYPT_SHA256_ALGORITHM, NULL, 0) != 0
        || BCryptCreateHash(sha->algorithm, &sha->hash, NULL, 0, NULL, 0, 0) != 0)
        rominabox_launch_die("could not check the game's files");
}

static void sha_add(Sha *sha, const void *bytes, size_t size) {
    BCryptHashData(sha->hash, (PUCHAR)bytes, (ULONG)size, 0);
}

static void sha_end(Sha *sha, unsigned char out[32]) {
    BCryptFinishHash(sha->hash, out, 32, 0);
    BCryptDestroyHash(sha->hash);
    BCryptCloseAlgorithmProvider(sha->algorithm, 0);
}

/* `path` in the form that Windows' file functions accept at any length.
 * After `\\?\`, a path is used exactly as written and is not limited to
 * MAX_PATH. The game's files are deep (in libretro's shader packs, some
 * files are over a hundred characters from their folder), and a folder that
 * we unpack or remove has a longer name than the game's, so we use the long
 * form for every file operation here. The player reads the unpacked game at
 * its ordinary path, which we keep within MAX_PATH in the export. */
static wchar_t *long_form(const wchar_t *path) {
    size_t length = wcslen(path) + 5;
    wchar_t *longer = malloc(length * sizeof *longer);
    if (!longer)
        rominabox_launch_die("out of memory");
    if (path[0] && path[1] == L':' && path[2] == L'\\')
        swprintf(longer, length, L"\\\\?\\%ls", path);
    else
        wcscpy(longer, path);
    return longer;
}

/* `path` without the `\\?\` of the long form, as we use it in the rest of
 * the launcher and in the player. */
static const wchar_t *ordinary(const wchar_t *path) {
    return wcsncmp(path, L"\\\\?\\", 4) == 0 ? path + 4 : path;
}

static wchar_t *joined(const wchar_t *folder, const char *relative) {
    wchar_t *tail = to_wide(relative);
    size_t length = wcslen(folder) + 1 + wcslen(tail) + 1;
    wchar_t *path = malloc(length * sizeof *path);
    if (!path)
        rominabox_launch_die("out of memory");
    swprintf(path, length, L"%ls\\%ls", folder, tail);
    for (wchar_t *cursor = path; *cursor; cursor++)
        if (*cursor == L'/')
            *cursor = L'\\';
    free(tail);
    return path;
}

/* Whether the file at `path` is `size` bytes whose hash is `expected`: all
 * of it, or its first and last 64 KB. */
static int holds(const wchar_t *path, uint64_t size, int whole, const unsigned char expected[32]) {
    WIN32_FILE_ATTRIBUTE_DATA facts;
    HANDLE file;
    Sha sha;
    unsigned char got[32];
    unsigned char *buffer;
    int same = 1;
    if (!GetFileAttributesExW(path, GetFileExInfoStandard, &facts)
        || (facts.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT))
        || (((uint64_t)facts.nFileSizeHigh << 32) | facts.nFileSizeLow) != size)
        return 0;
    file = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_FLAG_SEQUENTIAL_SCAN, NULL);
    buffer = malloc(CHUNK);
    if (file == INVALID_HANDLE_VALUE || !buffer) {
        if (file != INVALID_HANDLE_VALUE)
            CloseHandle(file);
        free(buffer);
        return 0;
    }
    sha_start(&sha);
    if (whole) {
        for (uint64_t left = size; left && same;) {
            DWORD want = left < CHUNK ? (DWORD)left : CHUNK;
            DWORD got_bytes = 0;
            same = ReadFile(file, buffer, want, &got_bytes, NULL) && got_bytes == want;
            sha_add(&sha, buffer, got_bytes);
            left -= got_bytes;
        }
    } else {
        DWORD head = size < END_BYTES ? (DWORD)size : END_BYTES;
        DWORD got_bytes = 0;
        LARGE_INTEGER where;
        same = ReadFile(file, buffer, head, &got_bytes, NULL) && got_bytes == head;
        sha_add(&sha, buffer, got_bytes);
        where.QuadPart = (LONGLONG)(size - head);
        same = same && SetFilePointerEx(file, where, NULL, FILE_BEGIN)
               && ReadFile(file, buffer, head, &got_bytes, NULL) && got_bytes == head;
        sha_add(&sha, buffer, got_bytes);
    }
    sha_end(&sha, got);
    CloseHandle(file);
    free(buffer);
    return same && memcmp(got, expected, 32) == 0;
}

/* Whether `folder` contains the whole game, as listed in the pack. */
static int unpacked(const wchar_t *folder, const Pack *pack) {
    wchar_t *program = joined(folder, pack->program);
    int whole = holds(program, pack->head_size, 1, pack->head_hash);
    free(program);
    for (uint32_t index = 0; whole && index < pack->count; index++) {
        const Entry *entry = &pack->entries[index];
        wchar_t *path = joined(folder, entry->path);
        whole = holds(path, entry->size, entry->read_whole, entry->read_whole ? entry->whole : entry->ends);
        free(path);
    }
    return whole;
}

/* Makes every folder on the way to `path`'s last part, below `root`. */
static void make_folders(const wchar_t *root, wchar_t *path) {
    for (wchar_t *cursor = path + wcslen(root) + 1; *cursor; cursor++) {
        if (*cursor != L'\\')
            continue;
        *cursor = L'\0';
        if (!CreateDirectoryW(path, NULL) && GetLastError() != ERROR_ALREADY_EXISTS)
            rominabox_launch_die("could not make the game's folder");
        *cursor = L'\\';
    }
}

typedef struct {
    UnpackDialog *dialog;
    uint64_t done;
    uint64_t total;
} Progress;

/* What we have written so far in an unpacking, and the file we are writing.
 * If unpacking stops at a damaged file or a full disk, we remove both when
 * the launcher ends (`forget_unpacking`). Otherwise a damaged game would
 * leave another partly unpacked copy at every launch. */
static struct {
    const wchar_t *folder;
    HANDLE file;
} unpacking = {NULL, INVALID_HANDLE_VALUE};

static void moved(Progress *progress, uint64_t bytes) {
    progress->done += bytes;
    unpack_dialog_progress(progress->dialog, progress->total ? (double)progress->done / progress->total : 1);
}

static HANDLE create(const wchar_t *root, const wchar_t *path) {
    wchar_t *folders = _wcsdup(path);
    HANDLE file;
    if (!folders)
        rominabox_launch_die("out of memory");
    make_folders(root, folders);
    free(folders);
    file = CreateFileW(path, GENERIC_WRITE, 0, NULL, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, NULL);
    if (file == INVALID_HANDLE_VALUE)
        rominabox_launch_die("could not unpack the game");
    unpacking.file = file;
    return file;
}

static void close_written(HANDLE file) {
    CloseHandle(file);
    unpacking.file = INVALID_HANDLE_VALUE;
}

static void write_all(HANDLE file, const void *bytes, size_t size) {
    DWORD wrote = 0;
    if (size && (!WriteFile(file, bytes, (DWORD)size, &wrote, NULL) || wrote != size))
        rominabox_launch_die("could not unpack the game: is the disk full?");
}

/* The launcher: the bytes of this program before the pack. */
static void unpack_program(HANDLE self, const Pack *pack, const wchar_t *folder, Progress *progress) {
    wchar_t *path = joined(folder, pack->program);
    HANDLE out = create(folder, path);
    unsigned char *buffer = malloc(CHUNK);
    unsigned char got[32];
    Sha sha;
    if (!buffer)
        rominabox_launch_die("out of memory");
    sha_start(&sha);
    for (uint64_t offset = 0; offset < pack->head_size;) {
        DWORD size = pack->head_size - offset < CHUNK ? (DWORD)(pack->head_size - offset) : CHUNK;
        read_at(self, offset, buffer, size);
        sha_add(&sha, buffer, size);
        write_all(out, buffer, size);
        offset += size;
        moved(progress, size);
    }
    sha_end(&sha, got);
    close_written(out);
    free(buffer);
    free(path);
    if (memcmp(got, pack->head_hash, 32) != 0)
        broken();
}

static void unpack_file(HANDLE self, ZSTD_DCtx *context, const Entry *entry, const wchar_t *folder,
                        Progress *progress) {
    wchar_t *path = joined(folder, entry->path);
    HANDLE out = create(folder, path);
    unsigned char *in_bytes = malloc(CHUNK);
    unsigned char *out_bytes = malloc(CHUNK);
    ZSTD_inBuffer in = {in_bytes, 0, 0};
    uint64_t left = entry->packed;
    uint64_t written = 0;
    unsigned char got[32];
    Sha sha;
    if (!in_bytes || !out_bytes)
        rominabox_launch_die("out of memory");
    ZSTD_DCtx_reset(context, ZSTD_reset_session_only);
    sha_start(&sha);
    for (;;) {
        ZSTD_outBuffer out_buffer = {out_bytes, CHUNK, 0};
        size_t hint;
        if (in.pos == in.size && left) {
            DWORD size = left < CHUNK ? (DWORD)left : CHUNK;
            read_at(self, entry->offset + (entry->packed - left), in_bytes, size);
            in.size = size;
            in.pos = 0;
            left -= size;
        }
        hint = ZSTD_decompressStream(context, &out_buffer, &in);
        if (ZSTD_isError(hint))
            broken();
        sha_add(&sha, out_bytes, out_buffer.pos);
        write_all(out, out_bytes, out_buffer.pos);
        written += out_buffer.pos;
        moved(progress, out_buffer.pos);
        if (hint == 0 && !left && in.pos == in.size)
            break;
        if (!out_buffer.pos && !left && in.pos == in.size)
            broken();
    }
    sha_end(&sha, got);
    close_written(out);
    free(in_bytes);
    free(out_bytes);
    free(path);
    if (written != entry->size || memcmp(got, entry->whole, 32) != 0)
        broken();
}

static void remove_tree(const wchar_t *path) {
    DWORD attributes = GetFileAttributesW(path);
    if (attributes == INVALID_FILE_ATTRIBUTES)
        return;
    if ((attributes & FILE_ATTRIBUTE_DIRECTORY) && !(attributes & FILE_ATTRIBUTE_REPARSE_POINT)) {
        size_t length = wcslen(path) + 3;
        wchar_t *pattern = malloc(length * sizeof *pattern);
        WIN32_FIND_DATAW found;
        HANDLE search;
        if (!pattern)
            return;
        swprintf(pattern, length, L"%ls\\*", path);
        search = FindFirstFileW(pattern, &found);
        free(pattern);
        if (search != INVALID_HANDLE_VALUE) {
            do {
                size_t child_length;
                wchar_t *child;
                if (!wcscmp(found.cFileName, L".") || !wcscmp(found.cFileName, L".."))
                    continue;
                child_length = wcslen(path) + 1 + wcslen(found.cFileName) + 1;
                child = malloc(child_length * sizeof *child);
                if (!child)
                    continue;
                swprintf(child, child_length, L"%ls\\%ls", path, found.cFileName);
                remove_tree(child);
                free(child);
            } while (FindNextFileW(search, &found));
            FindClose(search);
        }
        RemoveDirectoryW(path);
    } else if (attributes & FILE_ATTRIBUTE_DIRECTORY) {
        RemoveDirectoryW(path);
    } else {
        if (attributes & FILE_ATTRIBUTE_READONLY)
            SetFileAttributesW(path, attributes & ~FILE_ATTRIBUTE_READONLY);
        DeleteFileW(path);
    }
}

static void forget_unpacking(void) {
    if (unpacking.file != INVALID_HANDLE_VALUE)
        CloseHandle(unpacking.file);
    if (unpacking.folder)
        remove_tree(unpacking.folder);
}

void unpack_remove_tree(const wchar_t *path) {
    wchar_t *longer = long_form(path);
    remove_tree(longer);
    free(longer);
}

/* The game's folders beside `target`, which are earlier versions and the
 * leftovers of an interrupted unpacking, and `target` too unless `keep`. We
 * rename each before we remove it, and Windows refuses the rename while a
 * program in it runs, so we leave a version that is still running alone. */
static void forget_versions(const wchar_t *target, int keep) {
    const wchar_t *name = wcsrchr(target, L'\\');
    const wchar_t *version = name ? wcsrchr(name, L'-') : NULL;
    size_t folder_length;
    size_t identity_length;
    wchar_t *pattern;
    WIN32_FIND_DATAW found;
    HANDLE search;
    if (!name || !version)
        return;
    folder_length = (size_t)(name - target);
    identity_length = (size_t)(version - name);
    pattern = malloc((folder_length + identity_length + 3) * sizeof *pattern);
    if (!pattern)
        return;
    /* `<folder>\<identity>-*` */
    swprintf(pattern, folder_length + identity_length + 3, L"%.*ls%.*ls-*", (int)folder_length, target,
             (int)identity_length, name);
    search = FindFirstFileW(pattern, &found);
    free(pattern);
    if (search == INVALID_HANDLE_VALUE)
        return;
    do {
        size_t length = folder_length + 1 + wcslen(found.cFileName) + 32;
        wchar_t *other = malloc(length * sizeof *other);
        wchar_t *removing = malloc(length * sizeof *removing);
        if (other && removing && (found.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)
            && (!keep || wcscmp(found.cFileName, name + 1) != 0) && !wcsstr(found.cFileName, L".removing-")) {
            swprintf(other, length, L"%.*ls\\%ls", (int)folder_length, target, found.cFileName);
            swprintf(removing, length, L"%ls.removing-%lu", other, GetCurrentProcessId());
            if (MoveFileExW(other, removing, 0))
                remove_tree(removing);
        }
        free(other);
        free(removing);
    } while (FindNextFileW(search, &found));
    FindClose(search);
}

int unpack_game(const wchar_t *self, const char *local_app_data, int shown, char *folder, size_t folder_cap,
                wchar_t *program, size_t program_cap) {
    HANDLE file = CreateFileW(self, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_DELETE, NULL, OPEN_EXISTING,
                              FILE_ATTRIBUTE_NORMAL, NULL);
    Pack pack;
    wchar_t *root;
    wchar_t *target;
    wchar_t *launcher;
    int size;
    if (file == INVALID_HANDLE_VALUE)
        rominabox_launch_die("could not read the game's program");
    if (!find_pack(file, &pack)) {
        CloseHandle(file);
        return 0;
    }
    {
        wchar_t *given = to_wide(local_app_data);
        root = long_form(given);
        free(given);
    }
    target = joined(root, pack.runtime);

    if (!unpacked(target, &pack)) {
        wchar_t aside[32768];
        wchar_t fresh[32768];
        wchar_t title[260];
        UnpackLook look = {0};
        Progress progress = {0};
        ZSTD_DCtx *context = ZSTD_createDCtx();
        size_t length;
        if (!context)
            rominabox_launch_die("out of memory");
        /* We set aside a folder that is not complete, and never trust it. */
        if (GetFileAttributesW(target) != INVALID_FILE_ATTRIBUTES) {
            swprintf(aside, sizeof aside / sizeof aside[0], L"%ls.damaged-%llu", target,
                     (unsigned long long)GetTickCount64());
            MoveFileExW(target, aside, 0);
        }
        /* The folder itself, and every one on the way to it. */
        swprintf(fresh, sizeof fresh / sizeof fresh[0], L"%ls.unpacking-%lu\\", target, GetCurrentProcessId());
        make_folders(root, fresh);
        fresh[wcslen(fresh) - 1] = 0;
        unpacking.folder = fresh;
        atexit(forget_unpacking);

        MultiByteToWideChar(CP_UTF8, 0, pack.program, -1, title, sizeof title / sizeof title[0]);
        length = wcslen(title);
        if (length > 4 && _wcsicmp(title + length - 4, L".exe") == 0)
            title[length - 4] = L'\0';
        look.title = title;
        look.logo_png = pack.logo;
        look.logo_size = pack.logo_size;
        look.font_ttf = pack.font;
        look.font_size = pack.font_size;
        progress.total = pack.head_size;
        for (uint32_t index = 0; index < pack.count; index++)
            progress.total += pack.entries[index].size;
        progress.dialog = shown ? unpack_dialog_open(&look) : NULL;

        unpack_program(file, &pack, fresh, &progress);
        for (uint32_t index = 0; index < pack.count; index++)
            unpack_file(file, context, &pack.entries[index], fresh, &progress);
        ZSTD_freeDCtx(context);
        /* Complete, so we move it into place. If another launch was first, the
         * same game is already there. */
        if (!MoveFileExW(fresh, target, 0) && !unpacked(target, &pack))
            rominabox_launch_die("could not put the unpacked game in place");
        unpacking.folder = NULL;
        unpack_dialog_close(progress.dialog);
        forget_versions(target, 1);
    }
    CloseHandle(file);

    size = WideCharToMultiByte(CP_UTF8, 0, ordinary(target), -1, NULL, 0, NULL, NULL);
    if (size <= 0 || (size_t)size > folder_cap)
        rominabox_launch_die("a path does not fit");
    WideCharToMultiByte(CP_UTF8, 0, ordinary(target), -1, folder, size, NULL, NULL);
    launcher = joined(target, pack.program);
    if (wcslen(ordinary(launcher)) + 1 > program_cap)
        rominabox_launch_die("a path does not fit");
    wcscpy(program, ordinary(launcher));
    free(launcher);
    free(target);
    free(root);
    return 1;
}

void unpack_forget_all(const char *folder) {
    wchar_t *given = to_wide(folder);
    wchar_t *target = long_form(given);
    forget_versions(target, 0);
    free(target);
    free(given);
}
