/* Stand-ins for the rest of RetroArch: every name used in the sources that
 * a probe links (scripts/retroarch_probe.py). A probe never reaches these.
 * If it does, we stop the program with the name of the stand-in. The Windows
 * linker resolves every name, even in code that is never called, and the
 * macOS and GNU linkers drop that code first, so there these are unused.
 * Each is weak, so when a probe links the source that defines one, we use
 * that definition. We only stand in for functions here. A variable that a
 * linked source uses comes from the source that defines it. */
#include <stdio.h>
#include <stdlib.h>

#define UNREACHED(name) __attribute__((weak)) void name(void) { fprintf(stderr, "FAIL: reached " #name "\n"); abort(); }

UNREACHED(audio_get_bool_ptr)
UNREACHED(audio_get_float_ptr)
UNREACHED(audio_set_float)
UNREACHED(char_list_new_special)
UNREACHED(command_event)
UNREACHED(config_file_free)
UNREACHED(config_file_new_alloc)
UNREACHED(config_file_new_from_path_to_string)
UNREACHED(config_file_write)
UNREACHED(config_get_array)
UNREACHED(config_get_entry)
UNREACHED(config_get_path)
UNREACHED(config_get_ptr)
UNREACHED(config_set_path)
UNREACHED(dir_clear)
UNREACHED(dir_get_ptr)
UNREACHED(dir_set)
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
UNREACHED(path_get)
UNREACHED(path_is_directory)
UNREACHED(path_mkdir)
UNREACHED(path_set)
UNREACHED(RARCH_ERR)
UNREACHED(RARCH_LOG)
UNREACHED(RARCH_LOG_OUTPUT)
UNREACHED(RARCH_WARN)
UNREACHED(recording_state_get_ptr)
UNREACHED(retroarch_ctl)
UNREACHED(retroarch_override_setting_is_set)
UNREACHED(rib_rmlui_text_event)
UNREACHED(rtime_localtime)
UNREACHED(runloop_state_get_ptr)
UNREACHED(video_driver_get_threaded)
UNREACHED(video_driver_set_threaded)
