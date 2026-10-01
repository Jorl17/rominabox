#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/log_output.c points a process's own output at its log on macOS and Linux; the Windows launcher gives the player the log as its output"
#endif

#include "log_output.h"

#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

/* Anyone may read the launch log, and only its owner may write it. */
#define LOG_MODE 0644

int rominabox_output_to_log(const char *path) {
    int log_fd = open(path, O_WRONLY | O_CREAT | O_APPEND, LOG_MODE);
    if (log_fd < 0)
        return -1;
    dup2(log_fd, STDOUT_FILENO);
    dup2(log_fd, STDERR_FILENO);
    if (log_fd > STDERR_FILENO)
        close(log_fd);
    /* stdout is fully buffered when it is not a terminal. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    setvbuf(stderr, NULL, _IOLBF, 0);
    return 0;
}
