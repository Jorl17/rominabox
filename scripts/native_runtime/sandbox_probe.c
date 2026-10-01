/* A stand-in player for the isolation tests. We start it in the sandbox of
 * the exported game, in place of the player. For each thing that a game must
 * not do, we try it and print one word into the game's launch log. The file
 * checks are the same on every platform. The sandbox home folder, the names
 * of shared memory and the way we protect the network command port differ
 * between platforms. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include "windows/sandbox_probe.h"
#elif defined(__APPLE__) || defined(__unix__)
#include "posix/sandbox_probe.h"
#else
#error "the sandbox probe has no checks declared for this platform"
#endif

static void access_attempt(const char *env_name, const char *label, int writing) {
    const char *path = getenv(env_name);
    int fd;
    if (!path || !path[0]) {
        printf("%s_UNSET\n", label);
        return;
    }
    if (writing)
        fd = probe_open(path, O_WRONLY | O_CREAT | O_EXCL, 0644);
    else
        fd = probe_open(path, O_RDONLY);
    if (fd < 0) {
        printf("%s_DENIED\n", label);
        return;
    }
    printf("%s_ALLOWED\n", label);
    probe_close(fd);
}

/* The QUICK SIGN IN folder, whose path comes from the launcher, and a file
 * next to it, which must stay out of reach of the opened folder. */
static void accounts_attempt(void) {
    const char *accounts = getenv("ROMINABOX_ACCOUNTS_DIR");
    char path[4096];
    int fd;
    if (!accounts || !accounts[0]) {
        printf("ACCOUNTS_UNSET\n");
        return;
    }
    snprintf(path, sizeof path, "%s/probe-file", accounts);
    fd = probe_open(path, O_WRONLY | O_CREAT | O_TRUNC, 0600);
    printf(fd >= 0 ? "ACCOUNTS_ALLOWED\n" : "ACCOUNTS_DENIED\n");
    if (fd >= 0) {
        probe_close(fd);
        probe_unlink(path);
    }
}

/* RetroArch's network command port. */
#define NETWORK_COMMAND_PORT 55355

#if defined(_WIN32)
#include "windows/sandbox_attempts.h"
#elif defined(__APPLE__) || defined(__unix__)
#include "posix/sandbox_attempts.h"
#endif

int rarch_main(int argc, char **argv, void *data) {
    (void)argc;
    (void)argv;
    (void)data;
    print_homes();
    access_attempt("ROMINABOX_PROBE_READ", "READ", 0);
    access_attempt("ROMINABOX_PROBE_WRITE", "WRITE", 1);
    access_attempt("ROMINABOX_PROBE_OTHER", "OTHER", 0);
    accounts_attempt();
    access_attempt("ROMINABOX_PROBE_BESIDE", "BESIDE", 0);
    shared_memory_attempt();
    network_command_attempt();
    return 0;
}

int main(void) {
    return rarch_main(0, NULL, NULL);
}
