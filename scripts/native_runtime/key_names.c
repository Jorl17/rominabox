/* The names of keyboard keys in RetroArch, from the code of our fork: the
 * name table in input_keymaps.c and the parser for config files,
 * input_config_translate_str_to_rk in input_driver.c.
 *
 *   key_names              every key, by the name written for it in a
 *                          config file, one per line, in RetroArch's key order
 *   key_names NAME...      the key each NAME is parsed to, by its written
 *                          name, or nul when NAME is no key at all
 *
 * The first output is desktop/retroarch-keys.json, the only list of key
 * names allowed in a binding. With the second, we check a name against
 * RetroArch in a test. We call only the tables and the parser of RetroArch. */
#include <stdio.h>
#include <string.h>
#include <libretro.h>
#include "input/input_keymaps.h"
#include "input/input_remapping.h"

/* The RetroArch config name for `key`, or an empty string when there is none. */
static void name_of(enum retro_key key, char *name, size_t size)
{
   input_keymaps_translate_rk_to_str(key, name, size);
}

int main(int argc, char **argv)
{
   char name[64];
   int index;
   unsigned key;

   if (argc > 1)
   {
      for (index = 1; index < argc; ++index)
      {
         enum retro_key parsed = input_config_translate_str_to_rk(
               argv[index], strlen(argv[index]));
         name_of(parsed, name, sizeof(name));
         printf("%s\n", parsed == RETROK_UNKNOWN ? "nul" : name);
      }
      return 0;
   }

   /* We count a key when it has a RetroArch name that parses back to the
    * same key. RETROK_UNKNOWN is `nul`, which leaves a control unbound. */
   for (key = RETROK_UNKNOWN + 1; key < RETROK_LAST; ++key)
   {
      name_of((enum retro_key)key, name, sizeof(name));
      if (*name && input_config_translate_str_to_rk(name, strlen(name)) == key)
         printf("%s\n", name);
   }
   return 0;
}
