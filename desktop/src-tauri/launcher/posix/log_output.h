#ifndef ROMINABOX_LOG_OUTPUT_H
#define ROMINABOX_LOG_OUTPUT_H

/* Point this process's stdout and stderr at the end of the file at `path`,
 * created when missing, readable by anyone and writable by its owner. Make
 * both streams line-buffered, so each line is in the file once it is written,
 * and no line is lost when the player is killed or still running when someone
 * reads the log. Returns 0, or -1 with errno set when the file cannot be
 * opened, and then both streams stay as they were. Call it before anything is
 * written to either stream. */
int rominabox_output_to_log(const char *path);

#endif
