/* How we pass the prepared arguments to RetroArch's main in the launcher. In
 * the export we write a trampoline at the player's entry, in each processor's
 * slice (scripts/native_runtime/inject_dylib.c), with instructions that load
 * argc and argv from two slots and jump to main, and here we fill those
 * slots. A player without the trampoline keeps the arguments it started with. */
#ifndef ROMINABOX_LAUNCH_ARGUMENTS_H
#define ROMINABOX_LAUNCH_ARGUMENTS_H

void rominabox_publish_arguments(int argc, char **argv);

#endif
