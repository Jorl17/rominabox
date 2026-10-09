#include "game_data.h"

#include <errno.h>
#include <stddef.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "portable_fs.h"
#include "zip_library.h"

/* The folders a backup contains: the player's own, the settings an export
 * ships and the player can change, and what the launcher applied of them. */
static const char *const player_folders[] = {
#define RIB_PLAYER_FOLDER(name, path) path,
#include "launch_contract.inc"
#define RIB_SHIPPED_SETTINGS(name, app, data) data,
#include "launch_contract.inc"
#define RIB_GAME_DATA(name, path) path,
#include "launch_contract.inc"
};

/* The files the menu writes in a game's data. */
static const char *const menu_files[] = {
#define RIB_DATA_FILE(name, file) file,
#include "menu/drivers/rmlui/declarations.inc"
};

#define RIB_GAME_FILE(name, path) static const char game_file_##name[] = path;
#include "launch_contract.inc"

#define FOLDER_COUNT (sizeof player_folders / sizeof player_folders[0])
#define MENU_FILE_COUNT (sizeof menu_files / sizeof menu_files[0])

/* What we accept in a zip: no more entries, and no larger files, than any
 * game's data has. */
#define MOST_ENTRIES 100000
#define LARGEST_FILE (512ull * 1024 * 1024)
#define LARGEST_TOTAL (4ull * 1024 * 1024 * 1024)
#define DEEPEST_PATH 8
#define LONGEST_ENTRY 1024
#define LINE_SIZE 4096

static void say(char *error, size_t size, const char *format, ...) {
   va_list arguments;
   if (!error || !size)
      return;
   va_start(arguments, format);
   vsnprintf(error, size, format, arguments);
   va_end(arguments);
}

/* `text` without tabs and line breaks, which separate a manifest's fields. */
static void plain_copy(char *out, size_t size, const char *text) {
   size_t at = 0;
   for (; text && *text && at + 1 < size; text++)
      out[at++] = (*text == '\t' || *text == '\n' || *text == '\r') ? ' ' : *text;
   out[at] = '\0';
}

/* The manifest: one field per line, its name, a tab and its value, and one
 * `player_file` line for each player-setting file. */

typedef struct {
   const char *name;
   size_t offset;
   size_t size;
} Field;

#define FIELD(member) {#member, offsetof(rib_game_t, member), sizeof(((rib_game_t *)0)->member)}
static const Field fields[] = {
   FIELD(identity), FIELD(title), FIELD(system), FIELD(console), FIELD(content), FIELD(app), FIELD(made_with),
};
#define FIELD_COUNT (sizeof fields / sizeof fields[0])
static const char player_file_field[] = "player_file";

static const Field *field_named(const char *name) {
   size_t which;
   for (which = 0; which < FIELD_COUNT; which++)
      if (!strcmp(fields[which].name, name))
         return &fields[which];
   return NULL;
}

const char *rib_game_get(const rib_game_t *game, const char *name) {
   const Field *field = field_named(name);
   return field ? (const char *)game + field->offset : NULL;
}

int rib_game_set(rib_game_t *game, const char *name, const char *value) {
   const Field *field = field_named(name);
   if (field) {
      plain_copy((char *)game + field->offset, field->size, value);
      return 0;
   }
   if (!strcmp(name, player_file_field) && game->player_file_count < RIB_GAME_DATA_PLAYER_FILES) {
      plain_copy(game->player_files[game->player_file_count++], sizeof game->player_files[0], value);
      return 0;
   }
   return -1;
}

int rib_game_manifest_write(const char *data_dir, const rib_game_t *game) {
   char path[RIB_GAME_DATA_PATH_SIZE];
   char text[RIB_GAME_DATA_PATH_SIZE * 4];
   size_t used;
   size_t which;
   if (fs_join(path, sizeof path, data_dir, game_file_Manifest) != 0)
      return -1;
   used = (size_t)snprintf(text, sizeof text, "format\t%d\n", RIB_GAME_DATA_FORMAT);
   for (which = 0; which < FIELD_COUNT && used < sizeof text; which++)
      used += (size_t)snprintf(text + used, sizeof text - used, "%s\t%s\n", fields[which].name,
            (const char *)game + fields[which].offset);
   for (which = 0; which < game->player_file_count && used < sizeof text; which++)
      used += (size_t)snprintf(text + used, sizeof text - used, "%s\t%s\n", player_file_field,
            game->player_files[which]);
   if (used >= sizeof text) {
      errno = ENAMETOOLONG;
      return -1;
   }
   return fs_write_file(path, text, used);
}

/* Read a manifest from `text`, of `size` bytes. */
static int manifest_parse(const char *text, size_t size, rib_game_t *game) {
   size_t at = 0;
   int format = 0;
   memset(game, 0, sizeof *game);
   while (at < size) {
      char line[LINE_SIZE];
      size_t end = at;
      char *value;
      while (end < size && text[end] != '\n')
         end++;
      if (end - at < sizeof line) {
         memcpy(line, text + at, end - at);
         line[end - at] = '\0';
         if (end > at && line[end - at - 1] == '\r')
            line[end - at - 1] = '\0';
         value = strchr(line, '\t');
         if (value) {
            *value++ = '\0';
            if (!strcmp(line, "format"))
               format = atoi(value);
            else
               rib_game_set(game, line, value);
         }
      }
      at = end + 1;
   }
   if (format < 1 || format > RIB_GAME_DATA_FORMAT || !game->identity[0] || !game->system[0])
      return -1;
   return 0;
}

/* The whole file at `path` in memory, up to `largest` bytes, or NULL. */
static char *read_whole(const char *path, size_t largest, size_t *size) {
   FILE *file = fs_open(path, "rb");
   char *data = NULL;
   size_t used = 0;
   size_t capacity = 0;
   if (!file)
      return NULL;
   for (;;) {
      size_t got;
      if (used == capacity) {
         char *bigger;
         capacity = capacity ? capacity * 2 : 65536;
         if (capacity > largest + 1)
            capacity = largest + 1;
         if (used == capacity) {
            free(data);
            fclose(file);
            errno = EFBIG;
            return NULL;
         }
         bigger = realloc(data, capacity);
         if (!bigger) {
            free(data);
            fclose(file);
            return NULL;
         }
         data = bigger;
      }
      got = fread(data + used, 1, capacity - used, file);
      used += got;
      if (got == 0)
         break;
   }
   if (ferror(file)) {
      free(data);
      fclose(file);
      return NULL;
   }
   fclose(file);
   *size = used;
   return data ? data : calloc(1, 1);
}

int rib_game_manifest_read(const char *data_dir, rib_game_t *game) {
   char path[RIB_GAME_DATA_PATH_SIZE];
   size_t size = 0;
   char *text;
   int parsed;
   if (fs_join(path, sizeof path, data_dir, game_file_Manifest) != 0)
      return -1;
   text = read_whole(path, 1024 * 1024, &size);
   if (!text)
      return -1;
   parsed = manifest_parse(text, size, game);
   free(text);
   return parsed;
}

/* Which paths are a game's data */

static int is_player_folder(const char *name, size_t length) {
   size_t which;
   for (which = 0; which < FOLDER_COUNT; which++)
      if (strlen(player_folders[which]) == length && !strncmp(player_folders[which], name, length))
         return 1;
   return 0;
}

static int is_player_file(const char *name, const rib_game_t *game) {
   size_t which;
   for (which = 0; which < MENU_FILE_COUNT; which++)
      if (!strcmp(menu_files[which], name))
         return 1;
   for (which = 0; which < game->player_file_count; which++)
      if (!strcmp(game->player_files[which], name))
         return 1;
   return 0;
}

/* Whether `path`, from the top of a game's data with `/` between parts, is a
 * plain relative path: no part empty, `.` or `..`, nothing absolute, no
 * backslash, colon or control character, and not too deep. */
static int plain_path(const char *path) {
   size_t parts = 0;
   const char *part = path;
   if (!*path || strlen(path) >= LONGEST_ENTRY)
      return 0;
   for (;;) {
      const char *end = strchr(part, '/');
      size_t length = end ? (size_t)(end - part) : strlen(part);
      size_t at;
      if (!length || (length == 1 && part[0] == '.') || (length == 2 && part[0] == '.' && part[1] == '.'))
         return 0;
      for (at = 0; at < length; at++)
         if ((unsigned char)part[at] < 0x20 || part[at] == '\\' || part[at] == ':')
            return 0;
      if (++parts > DEEPEST_PATH)
         return 0;
      if (!end)
         return 1;
      part = end + 1;
   }
}

/* Whether `path` is part of the data of `game`: a file inside one of the
 * player's folders, a file the menu writes, or one of the game's player
 * files. */
static int in_game_data(const char *path, const rib_game_t *game) {
   const char *slash = strchr(path, '/');
   if (!plain_path(path))
      return 0;
   if (slash)
      return is_player_folder(path, (size_t)(slash - path)) && slash[1];
   return is_player_file(path, game);
}

/* `path` for a game whose saves are named after `to`, when they were named
 * after `from` in the backup: the file in a player's folder whose name is
 * `from` followed by nothing, `.`, `_` or `-` takes the name `to`. */
static void renamed(const char *path, const char *from, const char *to, char *out, size_t size) {
   const char *name = strrchr(path, '/');
   size_t length = strlen(from);
   name = name ? name + 1 : path;
   if (name != path && length && strcmp(from, to) && !strncmp(name, from, length) && strchr("._-", name[length])) {
      snprintf(out, size, "%.*s%s%s", (int)(name - path), path, to, name + length);
      return;
   }
   snprintf(out, size, "%s", path);
}

/* Writing a zip */

typedef struct {
   FILE *file;
} Output;

static int seek_to(FILE *file, mz_uint64 offset) {
#ifdef _WIN32
   return _fseeki64(file, (long long)offset, SEEK_SET);
#else
   return fseeko(file, (off_t)offset, SEEK_SET);
#endif
}

static size_t write_at(void *opaque, mz_uint64 offset, const void *data, size_t size) {
   Output *output = opaque;
   if (seek_to(output->file, offset) != 0)
      return 0;
   return fwrite(data, 1, size, output->file);
}

typedef struct {
   mz_zip_archive *zip;
   const char *data_dir;
   const char *prefix;
   char *error;
   size_t error_size;
} Adding;

static int add_tree(Adding *adding, const char *relative);

static int add_file(Adding *adding, const char *relative) {
   char path[RIB_GAME_DATA_PATH_SIZE];
   char entry[RIB_GAME_DATA_PATH_SIZE];
   size_t size = 0;
   char *data;
   MZ_TIME_T modified;
   if (fs_join(path, sizeof path, adding->data_dir, relative) != 0
         || (size_t)snprintf(entry, sizeof entry, "%s%s", adding->prefix, relative) >= sizeof entry) {
      say(adding->error, adding->error_size, "The path of “%s” is too long.", relative);
      return -1;
   }
   data = read_whole(path, LARGEST_FILE, &size);
   if (!data) {
      say(adding->error, adding->error_size, "We could not read “%s”: %s.", path, strerror(errno));
      return -1;
   }
   /* We keep the time of the last change, so the files of an imported
    * backup have the times they had in the game. */
   modified = (MZ_TIME_T)fs_modified(path);
   if (!mz_zip_writer_add_mem_ex_v2(adding->zip, entry, data, size, NULL, 0, MZ_DEFAULT_COMPRESSION, 0, 0,
             modified >= 0 ? &modified : NULL, NULL, 0, NULL, 0)) {
      free(data);
      say(adding->error, adding->error_size, "We could not add “%s” to the zip.", relative);
      return -1;
   }
   free(data);
   return 0;
}

typedef struct {
   Adding *adding;
   const char *folder;
} InFolder;

static int add_child(const char *name, void *context) {
   InFolder *in = context;
   char relative[RIB_GAME_DATA_PATH_SIZE];
   char path[RIB_GAME_DATA_PATH_SIZE];
   if ((size_t)snprintf(relative, sizeof relative, "%s/%s", in->folder, name) >= sizeof relative)
      return 0;
   if (!plain_path(relative))
      return 0;
   if (fs_join(path, sizeof path, in->adding->data_dir, relative) != 0)
      return 0;
   if (fs_is_directory(path))
      return add_tree(in->adding, relative);
   if (fs_is_file(path))
      return add_file(in->adding, relative);
   return 0;
}

static int add_tree(Adding *adding, const char *relative) {
   char path[RIB_GAME_DATA_PATH_SIZE];
   InFolder in;
   in.adding = adding;
   in.folder = relative;
   if (fs_join(path, sizeof path, adding->data_dir, relative) != 0)
      return 0;
   return fs_list(path, add_child, &in);
}

/* Add the data of the game in `data_dir` to `zip` under `prefix`. */
static int add_game(mz_zip_archive *zip, const char *data_dir, const char *prefix, const rib_game_t *game,
      char *error, size_t error_size) {
   Adding adding;
   size_t which;
   char path[RIB_GAME_DATA_PATH_SIZE];
   adding.zip = zip;
   adding.data_dir = data_dir;
   adding.prefix = prefix;
   adding.error = error;
   adding.error_size = error_size;
   if (add_file(&adding, game_file_Manifest) != 0)
      return -1;
   for (which = 0; which < FOLDER_COUNT; which++) {
      if (fs_join(path, sizeof path, data_dir, player_folders[which]) == 0 && fs_is_directory(path)
            && add_tree(&adding, player_folders[which]) != 0)
         return -1;
   }
   for (which = 0; which < MENU_FILE_COUNT; which++)
      if (fs_join(path, sizeof path, data_dir, menu_files[which]) == 0 && fs_is_file(path)
            && add_file(&adding, menu_files[which]) != 0)
         return -1;
   for (which = 0; which < game->player_file_count; which++)
      if (plain_path(game->player_files[which]) && !strchr(game->player_files[which], '/')
            && fs_join(path, sizeof path, data_dir, game->player_files[which]) == 0 && fs_is_file(path)
            && add_file(&adding, game->player_files[which]) != 0)
         return -1;
   return 0;
}

/* The title of `game`, with anything a file system may refuse in a name
 * replaced, in at most 63 bytes. */
static void safe_title(const rib_game_t *game, char *out, size_t size) {
   char title[64];
   size_t at = 0;
   const char *from = game->title[0] ? game->title : "Game";
   for (; *from && at + 1 < sizeof title; from++)
      title[at++] = (strchr("/\\:*?\"<>|", *from) || (unsigned char)*from < 0x20) ? '-' : *from;
   /* When we stop inside a character of UTF-8, we leave all of it out. */
   if (((unsigned char)*from & 0xC0) == 0x80) {
      while (at && ((unsigned char)title[at - 1] & 0xC0) == 0x80)
         at--;
      if (at)
         at--;
   }
   while (at && (title[at - 1] == ' ' || title[at - 1] == '.'))
      at--;
   title[at] = '\0';
   snprintf(out, size, "%s", title[0] ? title : "Game");
}

/* The folder of a game in a bulk backup: its title and the start of its
 * identity. */
static void game_folder(const rib_game_t *game, char *out, size_t size) {
   char title[64];
   safe_title(game, title, sizeof title);
   snprintf(out, size, "%s [%.8s]/", title, game->identity);
}

void rib_game_data_file_name(const rib_game_t *game, char *out, size_t size) {
   char title[64];
   safe_title(game, title, sizeof title);
   snprintf(out, size, "%s data.zip", title);
}

int rib_game_data_export(const char *const *data_dirs, size_t count, const char *zip_path,
      char *error, size_t error_size) {
   mz_zip_archive zip;
   Output output;
   char temporary[RIB_GAME_DATA_PATH_SIZE];
   size_t which;
   int failed = 0;
   if (!count || count > RIB_GAME_DATA_GAMES) {
      say(error, error_size, "Choose between 1 and %d games.", RIB_GAME_DATA_GAMES);
      return -1;
   }
   if ((size_t)snprintf(temporary, sizeof temporary, "%s.partial", zip_path) >= sizeof temporary) {
      say(error, error_size, "The path of the zip is too long.");
      return -1;
   }
   output.file = fs_open(temporary, "wb");
   if (!output.file) {
      say(error, error_size, "We could not write “%s”: %s.", zip_path, strerror(errno));
      return -1;
   }
   memset(&zip, 0, sizeof zip);
   zip.m_pWrite = write_at;
   zip.m_pIO_opaque = &output;
   if (!mz_zip_writer_init(&zip, 0)) {
      fclose(output.file);
      fs_remove(temporary);
      say(error, error_size, "We could not start the zip.");
      return -1;
   }
   for (which = 0; which < count && !failed; which++) {
      rib_game_t game;
      char prefix[RIB_GAME_DATA_TEXT_SIZE];
      if (rib_game_manifest_read(data_dirs[which], &game) != 0) {
         say(error, error_size, "The data in “%s” has no manifest yet. Open the game once, then try again.",
               data_dirs[which]);
         failed = 1;
         break;
      }
      prefix[0] = '\0';
      if (count > 1)
         game_folder(&game, prefix, sizeof prefix);
      failed = add_game(&zip, data_dirs[which], prefix, &game, error, error_size) != 0;
   }
   if (!failed && !mz_zip_writer_finalize_archive(&zip)) {
      say(error, error_size, "We could not finish the zip.");
      failed = 1;
   }
   mz_zip_writer_end(&zip);
   if (fclose(output.file) != 0 && !failed) {
      say(error, error_size, "We could not write “%s”: %s.", zip_path, strerror(errno));
      failed = 1;
   }
   if (!failed && fs_replace(temporary, zip_path) != 0) {
      say(error, error_size, "We could not write “%s”: %s.", zip_path, strerror(errno));
      failed = 1;
   }
   if (failed)
      fs_remove(temporary);
   return failed ? -1 : 0;
}

/* Reading a zip */

typedef struct {
   FILE *file;
} Input;

static size_t read_at(void *opaque, mz_uint64 offset, void *data, size_t size) {
   Input *input = opaque;
   if (seek_to(input->file, offset) != 0)
      return 0;
   return fread(data, 1, size, input->file);
}

/* A zip we have opened and checked: each game's folder in it (empty for a
 * game at its top) and manifest. */
typedef struct {
   mz_zip_archive zip;
   Input input;
   size_t count;
   char prefixes[RIB_GAME_DATA_GAMES][RIB_GAME_DATA_TEXT_SIZE];
   rib_game_t games[RIB_GAME_DATA_GAMES];
} Opened;

static void close_zip(Opened *opened) {
   mz_zip_reader_end(&opened->zip);
   if (opened->input.file)
      fclose(opened->input.file);
   free(opened);
}

static long long file_size(FILE *file) {
#ifdef _WIN32
   if (_fseeki64(file, 0, SEEK_END) != 0)
      return -1;
   return _ftelli64(file);
#else
   if (fseeko(file, 0, SEEK_END) != 0)
      return -1;
   return (long long)ftello(file);
#endif
}

/* The manifest at entry `index`, read into `game`. */
static int read_manifest_entry(mz_zip_archive *zip, mz_uint index, rib_game_t *game) {
   mz_zip_archive_file_stat stat;
   size_t size = 0;
   char *text;
   int parsed;
   if (!mz_zip_reader_file_stat(zip, index, &stat) || stat.m_uncomp_size > 1024 * 1024)
      return -1;
   text = mz_zip_reader_extract_to_heap(zip, index, &size, 0);
   if (!text)
      return -1;
   parsed = manifest_parse(text, size, game);
   mz_free(text);
   return parsed;
}

/* Open the zip at `path` and check every entry: each is a game's manifest,
 * or in the data of a game whose manifest is in the same folder. */
static Opened *open_zip(const char *path, char *error, size_t error_size) {
   Opened *opened = calloc(1, sizeof *opened);
   long long size;
   mz_uint entries;
   mz_uint index;
   mz_uint64 total = 0;
   size_t manifest_length = strlen(game_file_Manifest);
   if (!opened) {
      say(error, error_size, "There is not enough memory to read the zip.");
      return NULL;
   }
   opened->input.file = fs_open(path, "rb");
   if (!opened->input.file) {
      say(error, error_size, "We could not open “%s”: %s.", path, strerror(errno));
      free(opened);
      return NULL;
   }
   size = file_size(opened->input.file);
   opened->zip.m_pRead = read_at;
   opened->zip.m_pIO_opaque = &opened->input;
   if (size <= 0 || !mz_zip_reader_init(&opened->zip, (mz_uint64)size, 0)) {
      say(error, error_size, "This file is not a zip we can read.");
      close_zip(opened);
      return NULL;
   }
   entries = mz_zip_reader_get_num_files(&opened->zip);
   if (entries > MOST_ENTRIES) {
      say(error, error_size, "This zip contains more files than a game's data can have.");
      close_zip(opened);
      return NULL;
   }
   /* First the manifests: at the top for one game, or one per folder. */
   for (index = 0; index < entries; index++) {
      char name[LONGEST_ENTRY];
      size_t length;
      if (!mz_zip_reader_get_filename(&opened->zip, index, name, sizeof name))
         continue;
      length = strlen(name);
      if (length < manifest_length || strcmp(name + length - manifest_length, game_file_Manifest))
         continue;
      if (length > manifest_length && name[length - manifest_length - 1] != '/')
         continue;
      if (length > manifest_length && memchr(name, '/', length - manifest_length - 1))
         continue;
      if (opened->count == RIB_GAME_DATA_GAMES) {
         say(error, error_size, "This zip contains the data of more than %d games.", RIB_GAME_DATA_GAMES);
         close_zip(opened);
         return NULL;
      }
      snprintf(opened->prefixes[opened->count], sizeof opened->prefixes[0], "%.*s",
            (int)(length - manifest_length), name);
      if (read_manifest_entry(&opened->zip, index, &opened->games[opened->count]) != 0) {
         say(error, error_size, "This zip contains a manifest we cannot read, perhaps from a newer ROM-in-a-Box.");
         close_zip(opened);
         return NULL;
      }
      opened->count++;
   }
   {
      /* One game at the top, or every game in a folder of its own. */
      size_t game;
      int at_top = 0;
      for (game = 0; game < opened->count; game++)
         at_top |= !opened->prefixes[game][0];
      if (!opened->count || (opened->count > 1 && at_top)) {
         say(error, error_size, "This zip is not the data of a ROM-in-a-Box game.");
         close_zip(opened);
         return NULL;
      }
   }
   /* Then every entry, in the data of its game. */
   for (index = 0; index < entries; index++) {
      mz_zip_archive_file_stat stat;
      size_t game;
      int placed = 0;
      if (!mz_zip_reader_file_stat(&opened->zip, index, &stat)) {
         say(error, error_size, "This zip is damaged.");
         close_zip(opened);
         return NULL;
      }
      if (stat.m_is_directory)
         continue;
      total += stat.m_uncomp_size;
      if (!stat.m_is_supported || stat.m_is_encrypted || stat.m_uncomp_size > LARGEST_FILE || total > LARGEST_TOTAL) {
         say(error, error_size, "This zip contains “%s”, which we cannot import.", stat.m_filename);
         close_zip(opened);
         return NULL;
      }
      for (game = 0; game < opened->count && !placed; game++) {
         size_t length = strlen(opened->prefixes[game]);
         const char *relative = stat.m_filename + length;
         if (strncmp(stat.m_filename, opened->prefixes[game], length))
            continue;
         placed = !strcmp(relative, game_file_Manifest) || in_game_data(relative, &opened->games[game]);
      }
      if (!placed) {
         say(error, error_size, "This zip contains “%s”, which is not part of a game's data.",
               stat.m_filename);
         close_zip(opened);
         return NULL;
      }
   }
   return opened;
}

int rib_game_data_list(const char *zip_path, rib_game_t *games, size_t capacity, char *error, size_t error_size) {
   Opened *opened = open_zip(zip_path, error, error_size);
   size_t which;
   int count;
   if (!opened)
      return -1;
   for (which = 0; which < opened->count && which < capacity; which++)
      games[which] = opened->games[which];
   count = (int)opened->count;
   close_zip(opened);
   return count;
}

static rib_game_data_check_t check_opened(Opened *opened, size_t which, const rib_game_t *target,
      char *error, size_t error_size) {
   const rib_game_t *source;
   if (which >= opened->count) {
      say(error, error_size, "This zip does not contain that game.");
      return RIB_GAME_DATA_REFUSED;
   }
   source = &opened->games[which];
   if (strcmp(source->system, target->system)) {
      say(error, error_size, "This is the data of “%s”, a %s game, and this game is for the %s.",
            source->title, source->console, target->console);
      return RIB_GAME_DATA_REFUSED;
   }
   return strcmp(source->identity, target->identity) ? RIB_GAME_DATA_OTHER_GAME : RIB_GAME_DATA_SAME_GAME;
}

static int target_of(const char *data_dir, rib_game_t *target, char *error, size_t error_size) {
   if (rib_game_manifest_read(data_dir, target) == 0)
      return 0;
   say(error, error_size, "This game has no manifest yet. Open the game once, then try again.");
   return -1;
}

rib_game_data_check_t rib_game_data_check(const char *zip_path, size_t which, const char *data_dir,
      rib_game_t *source, char *error, size_t error_size) {
   rib_game_t target;
   Opened *opened;
   rib_game_data_check_t result;
   if (target_of(data_dir, &target, error, error_size) != 0)
      return RIB_GAME_DATA_REFUSED;
   opened = open_zip(zip_path, error, error_size);
   if (!opened)
      return RIB_GAME_DATA_REFUSED;
   result = check_opened(opened, which, &target, error, error_size);
   if (source && which < opened->count)
      *source = opened->games[which];
   close_zip(opened);
   return result;
}

/* Removing what an import replaces */

static int remove_tree(const char *path);

static int remove_child(const char *name, void *context) {
   char path[RIB_GAME_DATA_PATH_SIZE];
   if (fs_join(path, sizeof path, context, name) != 0)
      return 0;
   remove_tree(path);
   return 0;
}

/* Remove `path` with everything in it. We follow no link: we remove the
 * link itself. */
static int remove_tree(const char *path) {
   if (fs_is_directory(path)) {
      fs_list_all(path, remove_child, (void *)path);
      return fs_remove_directory(path);
   }
   return fs_remove(path);
}

/* Make the folders above the file at `relative` in `data_dir`. */
static int make_parents(const char *data_dir, const char *relative) {
   char partial[RIB_GAME_DATA_PATH_SIZE];
   char path[RIB_GAME_DATA_PATH_SIZE];
   const char *slash = relative;
   while ((slash = strchr(slash, '/'))) {
      snprintf(partial, sizeof partial, "%.*s", (int)(slash - relative), relative);
      if (fs_join(path, sizeof path, data_dir, partial) != 0 || fs_make_directory(path) != 0)
         return -1;
      slash++;
   }
   return 0;
}

static int import_opened(Opened *opened, size_t which, const char *data_dir, char *error, size_t error_size) {
   rib_game_t target;
   const rib_game_t *source;
   const char *prefix;
   size_t prefix_length;
   mz_uint entries = mz_zip_reader_get_num_files(&opened->zip);
   mz_uint index;
   size_t item;
   char path[RIB_GAME_DATA_PATH_SIZE];
   if (target_of(data_dir, &target, error, error_size) != 0
         || check_opened(opened, which, &target, error, error_size) == RIB_GAME_DATA_REFUSED)
      return -1;
   source = &opened->games[which];
   prefix = opened->prefixes[which];
   prefix_length = strlen(prefix);
   /* The zip is checked. We replace the player's data with the backup's. */
   for (item = 0; item < FOLDER_COUNT; item++)
      if (fs_join(path, sizeof path, data_dir, player_folders[item]) == 0)
         remove_tree(path);
   for (item = 0; item < MENU_FILE_COUNT; item++)
      if (fs_join(path, sizeof path, data_dir, menu_files[item]) == 0)
         fs_remove(path);
   for (item = 0; item < target.player_file_count; item++)
      if (plain_path(target.player_files[item]) && fs_join(path, sizeof path, data_dir, target.player_files[item]) == 0)
         fs_remove(path);
   for (index = 0; index < entries; index++) {
      mz_zip_archive_file_stat stat;
      char relative[RIB_GAME_DATA_PATH_SIZE];
      void *data;
      size_t size = 0;
      if (!mz_zip_reader_file_stat(&opened->zip, index, &stat) || stat.m_is_directory
            || strncmp(stat.m_filename, prefix, prefix_length))
         continue;
      if (!strcmp(stat.m_filename + prefix_length, game_file_Manifest))
         continue;
      /* We checked every entry when we opened the zip, and we check this
       * game's again before we write it. */
      if (!in_game_data(stat.m_filename + prefix_length, source))
         continue;
      /* A setting this game does not have. */
      if (!strchr(stat.m_filename + prefix_length, '/') && !is_player_file(stat.m_filename + prefix_length, &target))
         continue;
      renamed(stat.m_filename + prefix_length, source->content, target.content, relative, sizeof relative);
      if (!plain_path(relative) || make_parents(data_dir, relative) != 0
            || fs_join(path, sizeof path, data_dir, relative) != 0) {
         say(error, error_size, "We could not make the folder for “%s”.", relative);
         return -1;
      }
      data = mz_zip_reader_extract_to_heap(&opened->zip, index, &size, 0);
      if (!data) {
         say(error, error_size, "We could not read “%s” from the zip.", stat.m_filename);
         return -1;
      }
      if (fs_write_file(path, data, size) != 0) {
         say(error, error_size, "We could not write “%s”: %s.", path, strerror(errno));
         mz_free(data);
         return -1;
      }
      mz_free(data);
   }
   return 0;
}

int rib_game_data_import(const char *zip_path, size_t which, const char *data_dir, char *error, size_t error_size) {
   Opened *opened = open_zip(zip_path, error, error_size);
   int result;
   if (!opened)
      return -1;
   result = import_opened(opened, which, data_dir, error, error_size);
   close_zip(opened);
   return result;
}

/* An import chosen in the game */

int rib_game_data_set_aside(const char *zip_path, const char *data_dir, char *error, size_t error_size) {
   char path[RIB_GAME_DATA_PATH_SIZE];
   size_t size = 0;
   char *data;
   int written;
   if (fs_join(path, sizeof path, data_dir, game_file_PendingImport) != 0) {
      say(error, error_size, "The path of the game's data is too long.");
      return -1;
   }
   data = read_whole(zip_path, (size_t)LARGEST_TOTAL, &size);
   if (!data) {
      say(error, error_size, "We could not read “%s”: %s.", zip_path, strerror(errno));
      return -1;
   }
   written = fs_write_file(path, data, size);
   free(data);
   if (written != 0) {
      say(error, error_size, "We could not keep the zip for the next start: %s.", strerror(errno));
      return -1;
   }
   return 0;
}

/* The game in `opened` whose data a player who chose this zip in `target`'s
 * menu means: in a bulk backup, this game's, and else the zip's only game. */
static int pick_opened(const Opened *opened, const rib_game_t *target, char *error, size_t error_size) {
   size_t which;
   for (which = 0; which < opened->count; which++)
      if (!strcmp(opened->games[which].identity, target->identity))
         return (int)which;
   if (opened->count == 1)
      return 0;
   say(error, error_size, "This zip contains the data of several games, and none of them is “%s”.", target->title);
   return -1;
}

rib_game_data_check_t rib_game_data_choose(const char *zip_path, const char *data_dir, rib_game_t *source,
      char *error, size_t error_size) {
   rib_game_t target;
   Opened *opened;
   rib_game_data_check_t result = RIB_GAME_DATA_REFUSED;
   int which;
   if (target_of(data_dir, &target, error, error_size) != 0)
      return RIB_GAME_DATA_REFUSED;
   opened = open_zip(zip_path, error, error_size);
   if (!opened)
      return RIB_GAME_DATA_REFUSED;
   which = pick_opened(opened, &target, error, error_size);
   if (which >= 0) {
      result = check_opened(opened, (size_t)which, &target, error, error_size);
      if (source)
         *source = opened->games[which];
   }
   close_zip(opened);
   return result;
}

int rib_game_data_apply_pending(const char *data_dir, char *error, size_t error_size) {
   char path[RIB_GAME_DATA_PATH_SIZE];
   rib_game_t target;
   Opened *opened;
   int which;
   int result = -1;
   if (fs_join(path, sizeof path, data_dir, game_file_PendingImport) != 0 || !fs_is_file(path))
      return 0;
   opened = open_zip(path, error, error_size);
   if (opened && target_of(data_dir, &target, error, error_size) == 0
         && (which = pick_opened(opened, &target, error, error_size)) >= 0)
      result = import_opened(opened, (size_t)which, data_dir, error, error_size) == 0 ? 1 : -1;
   if (opened)
      close_zip(opened);
   fs_remove(path);
   return result;
}

/* Room for `count` games, for a caller that does not know the layout of
 * rib_game_t, such as the builder's Rust code. */
rib_game_t *rib_games_new(size_t count) {
   return calloc(count ? count : 1, sizeof(rib_game_t));
}

void rib_games_free(rib_game_t *games) {
   free(games);
}

rib_game_t *rib_games_at(rib_game_t *games, size_t which) {
   return games + which;
}

size_t rib_game_player_file_count(const rib_game_t *game) {
   return game->player_file_count;
}

const char *rib_game_player_file(const rib_game_t *game, size_t which) {
   return which < game->player_file_count ? game->player_files[which] : NULL;
}
