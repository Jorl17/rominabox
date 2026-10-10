/* The boundary to the driver host. We record the commands here, and the order
 * of document, focus, transfer, capture and configuration is the menu's own. */
#include "menu_host_fake.h"
#include "rmlui/declarations.h"
#include <algorithm>
#include <cstdarg>
#include <cstdio>
#include <cctype>
#include <cstring>

rib::test::FakeHost rib::test::host;
using rib::test::host;
using rib::test::Sound;

extern "C" bool rib_host_menu_open(void) { return host.menu_open; }
extern "C" void rib_host_open_menu(void) {}
extern "C" void rib_host_overlay_frames(bool) {}
extern "C" bool rib_host_has_settings(void) { return true; }
extern "C" bool rib_host_bind_index(const char *id, unsigned *index)
{
   if (!id || !*id) return false;
   auto found = std::find(host.bind_ids.begin(), host.bind_ids.end(), id);
   if (found == host.bind_ids.end())
   {
      host.bind_ids.emplace_back(id);
      found = host.bind_ids.end() - 1;
   }
   if (index) *index = static_cast<unsigned>(found - host.bind_ids.begin());
   return true;
}
extern "C" void rib_host_restore_keyboard_mapping(void) {}
extern "C" void rib_host_load_bind(struct config_file *, const char *id, unsigned)
{
   host.loaded_ids.emplace_back(id);
}
extern "C" void rib_host_clear_bind(unsigned) {}
extern "C" void rib_host_write_bind(struct config_file *, const char *, unsigned) {}
extern "C" bool rib_host_bind_conflicts(unsigned changed, unsigned other)
{
   return changed != other && changed < host.bind_ids.size()
         && host.bind_ids[changed] == host.clashing;
}
extern "C" void rib_host_bind_lines(unsigned index, char details[][64], char kinds[][8], int *lines)
{
   if (!details || !kinds || !lines || *lines + 2 > RIB_HOST_BIND_LINE_MAX) return;
   (void)index;
   std::snprintf(details[*lines], 64, "%s", host.bound_pad.c_str());
   std::strcpy(kinds[(*lines)++], "PAD");
   std::snprintf(details[*lines], 64, "%s", host.bound_key.c_str());
   std::strcpy(kinds[(*lines)++], "KEY");
}
extern "C" bool rib_host_capture_start(unsigned index, unsigned seconds)
{
   if (!host.capture_start_accepted) return false;
   host.captured_id = index < host.bind_ids.size() ? host.bind_ids[index] : "";
   host.capture_seconds = seconds;
   /* A new capture is pending until there is an input for it. */
   host.capture_result = RIB_CAPTURE_PENDING;
   if (host.timed_capture)
      host.capture_began_us = host.clock_us;
   ++host.captures_started;
   return true;
}
extern "C" void rib_host_capture_cancel(void) { ++host.captures_cancelled; }

namespace {
/* Keys in the order of their first mention, and pad inputs in the RetroPad
 * bind order, then home. */
std::vector<std::string> key_names;
const char *const pad_inputs[] = {"b", "y", "select", "start", "up", "down", "left",
      "right", "a", "x", "l", "r", "l2", "r2", "l3", "r3", "home"};
bool held(const std::vector<std::string>& down, const std::string& name)
{
   return std::find(down.begin(), down.end(), name) != down.end();
}
}
extern "C" bool rib_host_key_code(const char *name, unsigned *code)
{
   if (!name || !*name || !code || std::strchr(name, ' ')) return false;
   auto found = std::find(key_names.begin(), key_names.end(), name);
   if (found == key_names.end())
      found = key_names.insert(key_names.end(), name);
   *code = 1 + (unsigned)(found - key_names.begin());
   return true;
}
extern "C" bool rib_host_bind_key(unsigned, unsigned *code)
{
   return code && rib_host_key_code(host.bound_key.c_str(), code);
}
extern "C" bool rib_host_key_down(unsigned code)
{
   return code >= 1 && code <= key_names.size() && held(host.keys_down, key_names[code - 1]);
}
extern "C" bool rib_host_pad_input(const char *id, unsigned *bind)
{
   for (unsigned index = 0; id && index < sizeof(pad_inputs) / sizeof(pad_inputs[0]); ++index)
      if (!std::strcmp(id, pad_inputs[index]))
      {
         *bind = index;
         return true;
      }
   return false;
}
extern "C" bool rib_host_pad_down(unsigned bind)
{
   return bind < sizeof(pad_inputs) / sizeof(pad_inputs[0]) && held(host.pads_down, pad_inputs[bind]);
}
extern "C" bool rib_host_pad_value(const char *value)
{
   return value && (std::isdigit((unsigned char)value[0])
         || ((value[0] == '+' || value[0] == '-' || value[0] == 'h')
               && std::isdigit((unsigned char)value[1])));
}
extern "C" bool rib_host_pad_value_down(const char *value)
{
   return rib_host_pad_value(value) && held(host.pads_down, value);
}
extern "C" const char *rib_host_pad_input_id(unsigned bind)
{
   return bind < sizeof(pad_inputs) / sizeof(pad_inputs[0]) ? pad_inputs[bind] : nullptr;
}
extern "C" bool rib_host_pad_name(unsigned bind, char *name, size_t size)
{
   const char *id = rib_host_pad_input_id(bind);
   const auto found = id ? host.pad_names.find(id) : host.pad_names.end();
   if (found == host.pad_names.end() || !name || !size)
      return false;
   std::snprintf(name, size, "%s", found->second.c_str());
   return true;
}
extern "C" bool rib_host_pad_value_input(const char *value, unsigned *bind)
{
   const auto found = value ? host.pad_values.find(value) : host.pad_values.end();
   return found != host.pad_values.end() && rib_host_pad_input(found->second.c_str(), bind);
}
extern "C" bool rib_host_capture_input_start(unsigned seconds)
{
   if (!host.capture_start_accepted) return false;
   host.captured_id.clear();
   host.capture_seconds = seconds;
   host.capture_result = RIB_CAPTURE_PENDING;
   if (host.timed_capture)
      host.capture_began_us = host.clock_us;
   ++host.input_captures_started;
   return true;
}
extern "C" void rib_host_captured_input(char *binding, size_t length)
{
   if (binding && length)
      std::snprintf(binding, length, "%s", host.captured_input.c_str());
}
extern "C" rib_capture_result rib_host_capture_poll(bool accept_pointer, float *remaining)
{
   host.capture_accepts_pointer = accept_pointer;
   if (host.timed_capture)
   {
      const float left = (float)host.capture_seconds
            - (float)(host.clock_us - host.capture_began_us) / 1e6f;
      host.capture_remaining = left > 0.0f ? left : 0.0f;
      if (left <= 0.0f) host.capture_result = RIB_CAPTURE_TIMED_OUT;
   }
   if (remaining) *remaining = host.capture_remaining;
   return host.capture_result;
}
extern "C" rib_pointer rib_host_pointer(void) { return host.pointer; }
extern "C" int64_t rib_host_time_us(void) { return host.clock_us; }
extern "C" bool rib_host_core_gl_context(void) { return false; }
extern "C" bool rib_host_prepare_script_shot(void) { return false; }
extern "C" void rib_host_end_after_script_shot(const char *) {}
extern "C" void rib_host_script_finished(void) { host.script_finished = true; }
/* Pressed until the release in a test. */
extern "C" bool rib_host_script_press(const char *name)
{
   unsigned code = 0;
   if (!rib_host_key_code(name, &code)) return false;
   host.keys_down.emplace_back(name);
   return true;
}
extern "C" void rib_host_apply_device(const char *id, unsigned device)
{
   host.applied_device = id ? id : "";
   host.applied_libretro = device;
}
extern "C" unsigned rib_host_disc_count(void) { return host.disc_count; }
extern "C" unsigned rib_host_disc_index(void) { return host.disc_index; }
extern "C" void rib_host_disc_label(unsigned index, char *out, size_t length)
{
   if (!out || !length) return;
   if (index < host.disc_count) std::snprintf(out, length, "Disc %u", index + 1);
   else out[0] = '\0';
}
extern "C" void rib_host_choose_disc(unsigned index)
{
   if (index < host.disc_count) host.disc_index = index;
}
extern "C" bool rib_host_state_path(int slot, char *out, size_t length)
{
   if (slot != 1 || !out || host.state_path.size() + 1 > length) return false;
   std::strcpy(out, host.state_path.c_str());
   return true;
}
extern "C" bool rib_host_slot_occupied(int slot) { return slot == 1 && host.slot_occupied; }
extern "C" void rib_host_thumbnail(int slot, char *out, size_t length)
{
   if (out && length) out[0] = '\0';
   if (out && slot == 1 && host.slot_occupied && host.thumbnail.size() < length)
      std::strcpy(out, host.thumbnail.c_str());
}
extern "C" float rib_host_game_aspect(void) { return host.game_aspect; }
extern "C" void rib_host_select_state_slot(int slot) { host.selected_slot = slot; }
extern "C" bool rib_host_save_state(void)
{
   ++host.saves_started;
   return host.save_accepted;
}
extern "C" bool rib_host_copy_picture(int from, int to)
{
   host.picture_copies.emplace_back(from, to);
   return true;
}
extern "C" bool rib_host_load_state(void)
{
   ++host.loads_started;
   return host.load_accepted;
}
extern "C" void rib_host_resume(void) { ++host.resumes; }
extern "C" void rib_host_restart(void) { ++host.restarts; }
extern "C" void rib_host_toggle_fullscreen(void) { ++host.fullscreen_toggles; }
/* No pad rumbles here. */
extern "C" void rib_host_rumble_frame(void) {}
extern "C" void rib_host_video_pass(const char *pass, const char *written)
{
   host.video_pass = pass ? pass : "";
   host.video_written = written ? written : "";
}
extern "C" void rib_host_shader_brightness(const char *preset, const char *control)
{
   host.shader_brightness[preset ? preset : ""] = control ? control : "";
}
extern "C" void rib_host_show_pointer(bool) {}
extern "C" void rib_host_quit(void) { host.quit = true; }
extern "C" void rib_host_forget(void) { host.forgotten = true; }
extern "C" bool rib_host_setting(rib_setting_key key, float *value)
{
   const auto found = host.settings.find(rib::setting_key_name(key));
   if (found == host.settings.end() || !value) return false;
   *value = found->second;
   return true;
}
extern "C" bool rib_host_set_setting(rib_setting_key key, float value)
{
   const auto found = host.settings.find(rib::setting_key_name(key));
   if (found == host.settings.end()) return false;
   found->second = value;
   return true;
}
extern "C" bool rib_host_setting_used(rib_setting_key key)
{
   return key != RIB_SETTING_InputRumbleEnable || host.rumbles;
}
extern "C" void rib_host_level_sound(bool up)
{
   host.sounds.push_back(up ? Sound::LevelUp : Sound::LevelDown);
   host.level_cue_db.push_back(host.settings["audio_volume"]);
}
extern "C" void rib_host_load_level_cue(const char *path) { host.level_cue = path ? path : ""; }
extern "C" void rib_host_scroll_sound(bool up) { host.sounds.push_back(up ? Sound::ScrollUp : Sound::ScrollDown); }
extern "C" void rib_host_ok_sound(void) { host.sounds.push_back(Sound::Ok); }
extern "C" void rib_host_cancel_sound(void) { host.sounds.push_back(Sound::Cancel); }
extern "C" const char *rib_host_current_shader(void) { return host.current_shader.c_str(); }
extern "C" void rib_host_apply_shader(const char *id, const char *preset)
{
   if (host.drawing)
      ++host.applied_while_drawing;
   host.applied_shader = id ? id : "";
   host.applied_preset = preset ? preset : "";
   host.current_shader = host.applied_preset;
}

/* RetroArch's logging sink is outside the tested menu boundary. */
extern "C" void RARCH_LOG(const char *, ...) {}
extern "C" void RARCH_WARN(const char *, ...) {}
extern "C" void RARCH_ERR(const char *format, ...)
{
   char message[512];
   va_list args;
   va_start(args, format);
   std::vsnprintf(message, sizeof(message), format, args);
   va_end(args);
   host.error_log += message;
}
