/* Headless menu orchestration against the document, declarations and file layer
 * of the player. We replace only the RetroArch host commands and runtime state. */
#include "rmlui/menu_api.h"
#include "rmlui/host.h"
#include "rmlui_bridge.h"
#include <file/config_file.h>
#include "../../vendor/retroarch/audio/volume_range.h"

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdarg>
#include <string>
#include <fstream>
#include <vector>
#include <algorithm>

extern "C" const char *rib_rmlui_test_text(const char *id);
extern "C" bool rib_rmlui_test_has_class(const char *id, const char *name);

namespace {
int failures;
int captures_started;
int captures_cancelled;
int saves_started;
int loads_started;
int selected_slot;
bool slot_occupied;
bool save_accepted;
bool load_accepted;
rib_pointer pointer_state{};
int64_t clock_us;
std::string error_log;
std::vector<std::string> bind_ids, loaded_ids;
std::string captured_id;
rib_capture_result capture_result = RIB_CAPTURE_PENDING;
float capture_remaining = 9.0f;
bool capture_accepts_pointer;
float volume_db = -12.0f;
constexpr const char *state_path = "/headless-game/slot-1.state";

void check(bool condition, const char *message)
{
   if (!condition)
   {
      std::fprintf(stderr, "FAIL menu orchestration: %s\n", message);
      ++failures;
   }
}

void frame(void *menu) { rib_menu_frame(menu, 960, 600); }

void click_and_frame(void *menu, const char *id)
{
   check(rib_rmlui_click_element(id), id);
   frame(menu);
}

void hover_and_frame(void *menu, const char *id)
{
   check(rib_rmlui_element_center(id, &pointer_state.x, &pointer_state.y), id);
   frame(menu);
}

bool focused(const char *id)
{
   return rib_rmlui_test_has_class(id, "focused");
}

bool binds_visible(int *x, int *y)
{
   return rib_rmlui_element_center("control-binds", x, y)
         && rib_rmlui_pointer_inside("control-binds", *x, *y);
}

bool status_is(const char *expected)
{
   const char *status = rib_rmlui_test_text("status");
   return status && std::strcmp(status, expected) == 0;
}
}

/* The boundary to the driver host. We record the commands here, and the order
 * of document, focus, transfer, capture and configuration is the menu's own. */
extern "C" bool rib_host_menu_open(void) { return true; }
extern "C" void rib_host_overlay_frames(bool) {}
extern "C" bool rib_host_has_settings(void) { return true; }
extern "C" bool rib_host_bind_index(const char *id, unsigned *index)
{
   if (!id || !*id) return false;
   auto found = std::find(bind_ids.begin(), bind_ids.end(), id);
   if (found == bind_ids.end())
   {
      bind_ids.emplace_back(id);
      found = bind_ids.end() - 1;
   }
   if (index) *index = static_cast<unsigned>(found - bind_ids.begin());
   return true;
}
extern "C" void rib_host_restore_keyboard_mapping(void) {}
extern "C" void rib_host_load_bind(config_file *, const char *id, unsigned)
{
   loaded_ids.emplace_back(id);
}
extern "C" void rib_host_clear_bind(unsigned) {}
extern "C" void rib_host_write_bind(config_file *, const char *, unsigned) {}
extern "C" bool rib_host_bind_conflicts(unsigned, unsigned) { return false; }
extern "C" void rib_host_bind_lines(unsigned index, char details[][64], char kinds[][8], int *lines)
{
   if (!details || !kinds || !lines || *lines + 2 > RIB_HOST_BIND_LINE_MAX) return;
   std::snprintf(details[*lines], 64, "Button %u", index);
   std::strcpy(kinds[(*lines)++], "PAD");
   std::strcpy(details[*lines], "Key A");
   std::strcpy(kinds[(*lines)++], "KEY");
}
extern "C" bool rib_host_capture_start(unsigned index, unsigned)
{
   captured_id = index < bind_ids.size() ? bind_ids[index] : "";
   ++captures_started;
   return true;
}
extern "C" void rib_host_capture_cancel(void) { ++captures_cancelled; }
extern "C" rib_capture_result rib_host_capture_poll(bool accept_pointer, float *remaining)
{
   capture_accepts_pointer = accept_pointer;
   if (remaining) *remaining = capture_remaining;
   return capture_result;
}
extern "C" rib_pointer rib_host_pointer(void) { return pointer_state; }
extern "C" int64_t rib_host_time_us(void) { return clock_us; }
extern "C" bool rib_host_core_gl_context(void) { return false; }
extern "C" bool rib_host_prepare_script_shot(void) { return false; }
extern "C" void rib_host_end_after_script_shot(const char *) {}
extern "C" void rib_host_script_finished(void) {}
extern "C" void rib_host_apply_device(const char *, unsigned) {}
extern "C" unsigned rib_host_disc_count(void) { return 0; }
extern "C" unsigned rib_host_disc_index(void) { return 0; }
extern "C" void rib_host_disc_label(unsigned, char *out, size_t length)
{
   if (out && length) out[0] = '\0';
}
extern "C" void rib_host_choose_disc(unsigned) {}
extern "C" bool rib_host_state_path(int slot, char *out, size_t length)
{
   if (slot != 1 || !out || std::strlen(state_path) + 1 > length) return false;
   std::strcpy(out, state_path);
   return true;
}
extern "C" bool rib_host_slot_occupied(int slot) { return slot == 1 && slot_occupied; }
extern "C" void rib_host_thumbnail(int, char *out, size_t length)
{
   if (out && length) out[0] = '\0';
}
extern "C" float rib_host_game_aspect(void) { return 4.0f / 3.0f; }
extern "C" void rib_host_select_state_slot(int slot) { selected_slot = slot; }
extern "C" bool rib_host_save_state(void)
{
   ++saves_started;
   return save_accepted;
}
extern "C" bool rib_host_load_state(void)
{
   ++loads_started;
   return load_accepted;
}
extern "C" void rib_host_resume(void) {}
extern "C" void rib_host_quit(void) {}
extern "C" float rib_host_volume(void) { return volume_db; }
extern "C" bool rib_host_muted(void) { return false; }
extern "C" void rib_host_set_volume(float db) { volume_db = db; }
extern "C" void rib_host_scroll_sound(bool) {}
extern "C" void rib_host_ok_sound(void) {}
extern "C" void rib_host_cancel_sound(void) {}
extern "C" const char *rib_host_current_shader(void) { return ""; }
extern "C" void rib_host_apply_shader(const char *, const char *) {}

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
   error_log += message;
}

static int capacity_case(const char *assets, const char *data)
{
   setenv("ROMINABOX_RML_ASSETS", assets, 1);
   setenv("ROMINABOX_DATA_DIR", data, 1);
   unsetenv("ROMINABOX_MENU_SCRIPT");
   void *menu = rib_menu_create();
   check(menu != nullptr, "create menu for the large declared profile");
   if (menu)
   {
      frame(menu);
      rib_menu_toggle(menu, true);
      frame(menu);
      std::ifstream expected_file(std::string(assets) + "/expected-controls.txt");
      std::vector<std::string> expected;
      for (std::string id; std::getline(expected_file, id); ) expected.push_back(id);
      check(expected.size() == 48 && loaded_ids == expected,
            "49 declarations apply exactly the first 48 host bindings in declaration order");
      check(error_log.find("more than 48 controls declared") != std::string::npos,
            "the 49th declaration reports overflow");
      click_and_frame(menu, "options");
      click_and_frame(menu, "controls");
      check(rib_rmlui_has_element("control-group-l_stick"),
            "the staged 24-control document has its analogue direction group");
      for (int index = 0; index < 24 && index < static_cast<int>(expected.size()); ++index)
      {
         if (index == 16)
            check(focused("control-group-l_stick"),
                  "keyboard focus reaches the seventeenth declared control's group");
         rib_menu_key(menu, RIB_KEY_OK);
         check(captured_id == expected[index],
               ("keyboard capture addresses declared control " + expected[index]).c_str());
         rib_menu_key(menu, RIB_KEY_CANCEL);
         rib_menu_key(menu, RIB_KEY_DOWN);
      }
      rib_menu_destroy(menu);
   }
   if (failures)
      std::fprintf(stderr, "%d menu capacity failures\n", failures);
   return failures ? 1 : 0;
}

int main(int argc, char **argv)
{
   if (argc == 4 && std::strcmp(argv[1], "--capacity") == 0)
      return capacity_case(argv[2], argv[3]);
   if (argc != 3 || !argv[1][0] || !argv[2][0])
   {
      std::fprintf(stderr, "usage: %s staged-assets owned-data-dir\n"
            "       %s --capacity staged-large-profile owned-data-dir\n", argv[0], argv[0]);
      return 2;
   }
   setenv("ROMINABOX_RML_ASSETS", argv[1], 1);
   setenv("ROMINABOX_DATA_DIR", argv[2], 1);
   unsetenv("ROMINABOX_MENU_SCRIPT");

   void *menu = rib_menu_create();
   check(menu != nullptr, "create menu state");
   if (!menu) return 1;
   frame(menu);
   check(rib_rmlui_has_element("save"), "first frame loads the real document");
   rib_menu_toggle(menu, true);
   frame(menu);

   /* Immediate failures must clear the pending transfer, allowing a retry. */
   click_and_frame(menu, "save");
   check(saves_started == 1 && selected_slot == 1, "save selects slot and calls host once");
   check(status_is("SAVE FAILED"), "host save failure reaches the document");
   save_accepted = true;
   click_and_frame(menu, "save");
   check(saves_started == 2 && status_is("SAVING SLOT 1..."),
         "save retry waits for task completion");
   rib_rmlui_notify_state_task("/other-game/slot-1.state", 1, true, true);
   check(status_is("SAVING SLOT 1..."), "another game's task cannot finish this save");
   rib_rmlui_notify_state_task(state_path, 1, true, false);
   check(status_is("SAVE FAILED"), "matching asynchronous failure reaches the document");
   click_and_frame(menu, "save");
   check(saves_started == 3 && status_is("SAVING SLOT 1..."),
         "asynchronous failure also permits a fresh attempt");
   rib_rmlui_notify_state_task(state_path, 1, true, true);
   check(status_is("SLOT 1 SAVED"), "matching retry completion reports success");

   slot_occupied = true;
   frame(menu);
   load_accepted = false;
   click_and_frame(menu, "load");
   check(loads_started == 1 && status_is("LOAD FAILED"),
         "host load failure clears the transfer and reports failure");
   load_accepted = true;
   click_and_frame(menu, "load");
   check(loads_started == 2 && status_is("LOADING SLOT 1..."),
         "load retry waits for task completion");
   rib_rmlui_notify_state_task(state_path, 1, false, true);
   check(status_is("SLOT 1 LOADED"), "matching load completion reports success");

   click_and_frame(menu, "options");
   check(rib_rmlui_commit_slider(RIB_VOLUME_SLIDER_ID, 0.5f),
         "the staged Options screen exposes its volume slider");
   frame(menu);
   const std::string volume_path = std::string(argv[2]) + "/" + RIB_VOLUME_FILE;
   config_file_t *volume_file = config_file_new_from_path_to_string(volume_path.c_str());
   float saved_volume = 0.0f;
   check(volume_file && config_get_float(volume_file, RIB_VOLUME_KEY, &saved_volume)
         && std::fabs(saved_volume - volume_db) < 0.06f,
         "slider change persists through the real file/config layer");
   if (volume_file) config_file_free(volume_file);
   click_and_frame(menu, "controls");
   hover_and_frame(menu, "control-right");
   check(focused("control-right") && !focused("control-up"),
         "pointer focus paints only the hovered control");
   rib_menu_key(menu, RIB_KEY_DOWN);
   check(focused("control-down") && !focused("control-right"),
         "the next key continues from the control reached by pointer");

   clock_us = 0;
   hover_and_frame(menu, "control-up");
   int popup_x = 0, popup_y = 0;
   check(!binds_visible(&popup_x, &popup_y),
         "bind list remains closed before the declared delay");
   clock_us = 1200000;
   frame(menu);
   check(binds_visible(&popup_x, &popup_y),
         "bind list opens after its declared delay");
   hover_and_frame(menu, "controls-reset");
   check(focused("controls-reset") && !focused("control-up"),
         "pointer on Reset moves the one focus away from a control");
   check(!binds_visible(&popup_x, &popup_y),
         "Reset hover closes the bind list");
   hover_and_frame(menu, "control-up");
   clock_us = 2400000;
   frame(menu);
   check(binds_visible(&popup_x, &popup_y),
         "bind list can reopen after Reset");
   pointer_state.x = popup_x;
   pointer_state.y = popup_y;
   frame(menu);
   check(focused("control-up")
         && binds_visible(&popup_x, &popup_y),
         "pointer over the popup retains its source control and the popup");
   pointer_state.x = 0;
   pointer_state.y = 0;
   frame(menu);
   check(focused("control-up"), "pointer leaving retains keyboard focus");
   rib_menu_key(menu, RIB_KEY_DOWN);
   check(focused("control-left") && !focused("control-up"),
         "keyboard navigation continues from the retained focus");
   hover_and_frame(menu, "controls-back");
   check(focused("controls-back") && !focused("control-left"),
         "pointer on Back moves the one focus away from a control");
   check(!binds_visible(&popup_x, &popup_y),
         "Back hover closes the bind list");

   click_and_frame(menu, "controls-back");
   click_and_frame(menu, "options");
   click_and_frame(menu, "fixture");
   hover_and_frame(menu, "fixture-two");
   check(focused("fixture-two") && !focused("fixture-one"),
         "list hover gives only the hovered row its focus outline");
   rib_menu_key(menu, RIB_KEY_UP);
   check(focused("fixture-one") && !focused("fixture-two"),
         "list keyboard navigation continues from the hovered row");
   click_and_frame(menu, "fixture-back");
   click_and_frame(menu, "controls");
   pointer_state.x = 0;
   pointer_state.y = 0;
   pointer_state.pressed = true;
   click_and_frame(menu, "control-up");
   check(captures_started == 1, "control click starts capture through fake host");
   check(!capture_accepts_pointer, "the opening pointer gesture is excluded from capture");
   capture_remaining = 5.0f;
   frame(menu);
   check(std::string(rib_rmlui_test_text("controls-status")) == "up: PRESS AN INPUT (5)",
         "capture countdown remains visible while the opening pointer is held");
   capture_remaining = 4.0f;
   frame(menu);
   check(std::string(rib_rmlui_test_text("controls-status")) == "up: PRESS AN INPUT (4)",
         "capture countdown continues while pointer input is gated");
   capture_result = RIB_CAPTURE_TIMED_OUT;
   capture_remaining = 0.0f;
   frame(menu);
   check(std::string(rib_rmlui_test_text("controls-status")) == "TIMED OUT; BINDING UNCHANGED",
         "host timeout ends capture even while the opening pointer is held");
   pointer_state.pressed = false;
   capture_result = RIB_CAPTURE_PENDING;
   capture_remaining = 9.0f;
   frame(menu);
   click_and_frame(menu, "control-up");
   check(captures_started == 2, "a fresh capture can start after timeout");
   frame(menu);
   check(capture_accepts_pointer, "pointer release enables capture input on the next frame");
   rib_menu_destroy(menu);
   check(captures_cancelled == 1, "destruction cancels live capture");
   check(!rib_rmlui_has_element("save"), "destruction releases the document");
   rib_rmlui_notify_state_task(state_path, 1, true, true);

   menu = rib_menu_create();
   check(menu != nullptr, "create after destruction");
   if (menu)
   {
      frame(menu);
      check(rib_rmlui_has_element("save"), "new menu loads after prior destruction");
      rib_menu_toggle(menu, true);
      rib_menu_context_destroy(menu);
      check(!rib_rmlui_has_element("save"), "context destruction releases document");
      rib_menu_context_reset(menu);
      frame(menu);
      check(rib_rmlui_has_element("save"), "context reset reloads the document");
      rib_menu_destroy(menu);
   }

   if (failures)
      std::fprintf(stderr, "%d menu orchestration failures\n", failures);
   return failures ? 1 : 0;
}
