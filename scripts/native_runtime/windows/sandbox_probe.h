#pragma once

#ifndef _WIN32
#error "windows/sandbox_probe.h is the headers and file calls the sandbox probe uses on Windows; sandbox_probe.c names each platform's"
#endif

#include <winsock2.h>
#include <windows.h>
#include <io.h>
#define probe_open _open
#define probe_close _close
#define probe_unlink _unlink
