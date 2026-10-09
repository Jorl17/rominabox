/* miniz, as every program that writes or reads a game's data compiles it
 * (zip_library.c): without its file functions, because we read and write
 * through the launcher's file layer for UTF-8 paths on every platform, and
 * without zlib's names, so a program can link zlib as well. */
#ifndef ROMINABOX_ZIP_LIBRARY_H
#define ROMINABOX_ZIP_LIBRARY_H

#define MINIZ_NO_STDIO
#define MINIZ_NO_ZLIB_COMPATIBLE_NAMES
#include "miniz.h"

#endif
