/* Print the decision of the launcher about an automated launch:
 *
 *   quiet_decision person|harness QUIET SOUND
 *
 * prints "quiet" or "sound". QUIET and SOUND are the switch and the opt-out
 * that we set in the harness, empty when unset. We decide in the entry point
 * of each platform whether a person made the launch (Launch Services on
 * macOS, Explorer on Windows). We link only the shared launcher sources. */
#include <stdio.h>
#include <string.h>

#include "../../desktop/src-tauri/launcher/launch.h"

/* On each platform, we show a person why a game cannot start in the entry
 * point. This tool has no entry point, so we show nothing. */
void rominabox_launch_tell(const char *message) {
    (void)message;
}

int main(int argc, char **argv) {
    const int person = argc > 1 && strcmp(argv[1], "person") == 0;
    const char *quiet = argc > 2 && argv[2][0] ? argv[2] : NULL;
    const char *sound = argc > 3 && argv[3][0] ? argv[3] : NULL;
    printf("%s\n", rominabox_launch_is_quiet(person, quiet, sound) ? "quiet" : "sound");
    return 0;
}
