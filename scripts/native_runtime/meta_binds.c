/* Print every RetroArch meta bind and its default key, from the tables in
 * the fork: input_config_bind_map in configuration.c, the desktop defaults
 * in config.def.keybinds.h, and the RetroArch names for keys in
 * input_keymaps.c. One line per bind, in the order of the table:
 *
 *   <bind> <default key, or nul>
 *
 * We check the exporter's hotkey policy (desktop/src-tauri/src/hotkeys.rs)
 * against this. No RetroArch code runs: we only read and print the tables. */
#include <stdio.h>
#include <libretro.h>
#include "input/input_defines.h"
#include "input/input_types.h"
#include "input/input_keymaps.h"
#include "config.def.keybinds.h"

/* The default key for `id` in RetroArch, by its name in the config. */
static const char *default_key(unsigned id)
{
   size_t bind, key;
   for (bind = 0; bind < sizeof(retro_keybinds_1) / sizeof(retro_keybinds_1[0]); ++bind)
      if (retro_keybinds_1[bind].id == id)
         for (key = 0; input_config_key_map[key].str; ++key)
            if (input_config_key_map[key].key == retro_keybinds_1[bind].key)
               return input_config_key_map[key].str;
   return "nul";
}

int main(void)
{
   unsigned index;
   for (index = RARCH_FIRST_META_KEY; index < RARCH_BIND_LIST_END; ++index)
   {
      const struct input_bind_map *bind = INPUT_CONFIG_BIND_MAP_GET(index);
      if (bind->valid && bind->base)
         printf("%s %s\n", bind->base, default_key(bind->retro_key));
   }
   return 0;
}
