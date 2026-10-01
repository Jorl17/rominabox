#pragma once

#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/sandbox_probe.h is the headers and file calls the sandbox probe uses on macOS and Linux; sandbox_probe.c names each platform's"
#endif

#include <netinet/in.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <unistd.h>
#define probe_open open
#define probe_close close
#define probe_unlink unlink
