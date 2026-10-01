/* A process with its output sent to a log in the same way as the player's
 * output on macOS (launcher/posix/log_output.c). Write one line and exit
 * without flushing, so that the line is in the log only when every line
 * goes to the file at the moment it is written.
 *
 *     log_lines_probe LOG */
#include <stdio.h>
#include <unistd.h>

#include "log_output.h"

int main(int argc, char **argv) {
    if (argc != 2)
        return 2;
    if (rominabox_output_to_log(argv[1]) != 0)
        return 3;
    printf("menu line\n");
    _exit(0);
}
