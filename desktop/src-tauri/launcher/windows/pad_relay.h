/* Inside its sandbox, a Windows game has no access to the controllers in
 * DirectInput, so we read them in the launcher, outside the sandbox
 * (vendor/retroarch/rominabox_pad_relay.h), once per frame on request, as in
 * RetroArch's own code. */
#ifndef ROMINABOX_LAUNCHER_PAD_RELAY_H
#define ROMINABOX_LAUNCHER_PAD_RELAY_H

typedef struct PadRelay PadRelay;

/* Find the controllers, start answering requests for them, and set the name
 * of the relay in the environment of the game. NULL, with no name set, when
 * there is nothing to answer with. The game then has no DirectInput
 * controllers, as in a sandbox without the relay. */
PadRelay *pad_relay_start(void);

/* Stop answering, once the game has ended. */
void pad_relay_stop(PadRelay *relay);

#endif
