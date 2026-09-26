/* How RetroArch parses a key name, with the code of our fork: the parser for
 * config files, input_config_translate_str_to_rk in input_driver.c, and the
 * key table in input_keymaps.c, made from input/input_key_names.inc.
 *
 *   key_names NAME...      for each NAME, one line: the config name of the
 *                          key that NAME is parsed to, or nul when NAME is
 *                          no key at all
 *
 * In the exporter tests we check with it the key names stored by the builder
 * and the default keys of the controllers. We call only the table and the
 * parser of RetroArch. */
#include <stdio.h>
#include <string.h>
#include <libretro.h>
#include "input/input_keymaps.h"
#include "input/input_remapping.h"

int main(int argc, char **argv)
{
   char name[64];
   int index;

   for (index = 1; index < argc; ++index)
   {
      enum retro_key read = input_config_translate_str_to_rk(
            argv[index], strlen(argv[index]));
      input_keymaps_translate_rk_to_str(read, name, sizeof(name));
      printf("%s\n", read == RETROK_UNKNOWN ? "nul" : name);
   }
   return 0;
}
