/* Inside its sandbox, a Windows game has no access to the controllers in
 * DirectInput, so we read them in the launcher, outside the sandbox
 * (vendor/retroarch/rominabox_pad_relay.h), once per frame on request, as in
 * RetroArch's own code. Through the same relay the game asks us to export
 * and import its data (game_data_requests.h). */
#ifndef ROMINABOX_LAUNCHER_PAD_RELAY_H
#define ROMINABOX_LAUNCHER_PAD_RELAY_H

typedef struct PadRelay PadRelay;

/* Start answering requests for the game's controllers and data, whose
 * folder, as the game sees it, is `data_dir`, and set the name of the relay
 * in the environment of the game. NULL, with no name set, when there is
 * nothing to answer with. The game then has no DirectInput controllers, as
 * in a sandbox without the relay. */
PadRelay *pad_relay_start(const char *data_dir);

/* Stop answering, once the game has ended. */
void pad_relay_stop(PadRelay *relay);

#endif
