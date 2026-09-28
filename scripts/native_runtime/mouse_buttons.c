/* How RetroArch reads each mouse button that a game declares
 * (menu/drivers/rmlui/mouse_buttons.inc), with the code in the fork: the
 * parser for input_player1_<control>_mbtn in a controls file,
 * input_config_parse_mouse_button in configuration.c.
 *
 *   mouse_buttons      one line per declared button: its value, then "ok"
 *                      when RetroArch reads it as the declared button, or
 *                      the id that RetroArch reads instead
 *
 * We run it in the exporter tests on the values that the builder stores. We
 * call only the parser, and no other RetroArch code runs. */
#include <stdio.h>
#include <libretro.h>
#include <file/config_file.h>
#include "configuration.h"
#include "input/input_driver.h"

static void check(const char *value, unsigned id)
{
   char base[] = "input_player1_b";
   struct retro_keybind bind;
   config_file_t *config = config_file_new_alloc();
   bind.mbutton = NO_BTN;
   config_set_string(config, "input_player1_b_mbtn", value);
   input_config_parse_mouse_button(base, config, "input_player1", "b", &bind);
   if (bind.mbutton == id)
      printf("%s ok\n", value);
   else
      printf("%s reads as %u\n", value, (unsigned)bind.mbutton);
   config_file_free(config);
}

int main(void)
{
#define RIB_MOUSE_BUTTON(value, id, word) check(value, id);
#include "menu/drivers/rmlui/mouse_buttons.inc"
   return 0;
}
