/* Stand-ins for the rest of RetroArch, for every name in input_driver.c.
 * The functions that test_menu_typing.c calls never reach these. If one does,
 * we stop the program with the name of the stand-in. The Windows linker
 * resolves every name, even in uncalled code, and the macOS and GNU linkers
 * drop that code first, so there these are unused. */
#include <stdio.h>
#include <stdlib.h>

#define UNREACHED(name) void name(void) { fprintf(stderr, "FAIL: reached " #name "\n"); abort(); }

UNREACHED(RARCH_ERR)
UNREACHED(RARCH_LOG)
UNREACHED(RARCH_LOG_OUTPUT)
UNREACHED(RARCH_WARN)
UNREACHED(char_list_new_special)
UNREACHED(command_event)
UNREACHED(config_get_entry)
UNREACHED(config_get_ptr)
UNREACHED(driver_find_index)
UNREACHED(input_autoconfigure_connect)
UNREACHED(input_config_bind_map)
UNREACHED(input_config_bind_map_get_valid)
UNREACHED(input_config_get_prefix)
UNREACHED(input_config_get_sensor_map)
UNREACHED(input_config_parse_joy_axis)
UNREACHED(input_config_parse_joy_button)
UNREACHED(input_config_parse_mouse_button)
UNREACHED(input_config_reset_autoconfig_binds)
UNREACHED(input_remapping_save_file)
UNREACHED(menu_entry_get)
UNREACHED(msg_hash_to_str)
UNREACHED(retroarch_override_setting_is_set)
UNREACHED(rib_rmlui_text_event)
UNREACHED(rtime_localtime)
UNREACHED(runloop_state_get_ptr)
