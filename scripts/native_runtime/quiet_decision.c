/* Print the decision of the launcher about a launch:
 *
 *   quiet_decision QUIET
 *
 * prints "quiet" or "sound". QUIET is the switch that we set in a harness,
 * empty when unset. We link only the shared launcher sources. */
#include <stdio.h>

#include "../../desktop/src-tauri/launcher/launch.h"

/* On each platform, we show a person why a game cannot start in the entry
 * point. This tool has no entry point, so we show nothing. */
void rominabox_launch_tell(const char *message) {
    (void)message;
}

int main(int argc, char **argv) {
    const char *quiet = argc > 1 && argv[1][0] ? argv[1] : NULL;
    printf("%s\n", rominabox_launch_is_quiet(quiet) ? "quiet" : "sound");
    return 0;
}
