/* Headless menu orchestration against the document, declarations and file layer
 * of the player. We replace only the RetroArch host commands and runtime state. */
#include "rmlui/menu_api.h"
#include "rmlui/host.h"
#include "rmlui_bridge.h"
#include "rmlui/view.hpp"
#include "rmlui/elements.hpp"
#include "rmlui/overlays.hpp"
#include "menu_test_view.hpp"
#include "menu_host_fake.h"
#include <file/config_file.h>
#include "../../vendor/retroarch/audio/volume_range.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <fstream>
#include <vector>


namespace {
rib::View& view = rib::menu_view();
rib::test::Inspection inspect(view.document);
using rib::test::host;
int failures;

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
   check(view.document.click_element(id), id);
   frame(menu);
}

void hover_and_frame(void *menu, const char *id)
{
   check(view.document.element_center(id, &host.pointer.x, &host.pointer.y), id);
   frame(menu);
}

bool focused(const char *id)
{
   return inspect.has_class(id, "focused");
}

bool binds_visible(int *x, int *y)
{
   return view.document.element_center("control-binds", x, y)
         && view.document.pointer_inside("control-binds", *x, *y);
}

bool status_is(const char *expected)
{
   const char *status = inspect.text("status");
   return status && std::strcmp(status, expected) == 0;
}
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
      check(expected.size() == 48 && host.loaded_ids == expected,
            "49 declarations apply exactly the first 48 host bindings in declaration order");
      check(host.error_log.find("more than 48 controls declared") != std::string::npos,
            "the 49th declaration reports overflow");
      click_and_frame(menu, "options");
      click_and_frame(menu, "controls");
      check(view.document.has_element("control-group-l_stick"),
            "the staged 24-control document has its analogue direction group");
      /* Every stop in the scene captures the control it stands for: a callout
       * captures its control, and the box of a stick (one stop for the stick)
       * its first member. We reach each by pointer and press it by keyboard. */
      std::vector<std::string> stops;
      rib::walk(view.document.root()->GetElementById("controller-scene"), [&](Rml::Element *element) {
         if (!element->IsClassSet("control-callout") && !element->IsClassSet("control-group"))
            return rib::Walk::Continue;
         stops.push_back(element->GetId());
         return rib::Walk::SkipChildren;
      });
      check(stops.size() == 16, "the 24 declared controls are 14 callouts and 2 sticks");
      std::vector<std::string> captured;
      for (const std::string& stop : stops)
      {
         hover_and_frame(menu, stop.c_str());
         check(focused(stop.c_str()), ("the pointer focuses " + stop).c_str());
         rib_menu_key(menu, RIB_KEY_OK);
         const bool callout = stop.rfind("control-group-", 0) != 0;
         check(!callout || host.captured_id == stop.substr(std::strlen("control-")),
               ("keyboard capture addresses the control " + stop + " draws").c_str());
         check(std::find(expected.begin(), expected.begin() + 24, host.captured_id)
                     != expected.begin() + 24
               && std::find(captured.begin(), captured.end(), host.captured_id) == captured.end(),
               ("each stop captures a different declared control; " + stop).c_str());
         captured.push_back(host.captured_id);
         rib_menu_key(menu, RIB_KEY_CANCEL);
      }
      hover_and_frame(menu, "control-group-l_stick");
      rib_menu_key(menu, RIB_KEY_OK);
      check(host.captured_id == expected[14],
            "the left stick's stop captures its first declared member");
      rib_menu_key(menu, RIB_KEY_CANCEL);
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
   check(view.document.has_element("save"), "first frame loads the real document");
   rib_menu_toggle(menu, true);
   frame(menu);

   /* Immediate failures must clear the pending transfer, allowing a retry. */
   click_and_frame(menu, "save");
   check(host.saves_started == 1 && host.selected_slot == 1, "save selects slot and calls host once");
   check(status_is("SAVE FAILED"), "host save failure reaches the document");
   host.save_accepted = true;
   click_and_frame(menu, "save");
   check(host.saves_started == 2 && status_is("SAVING SLOT 1..."),
         "save retry waits for task completion");
   rib_rmlui_notify_state_task("/other-game/slot-1.state", 1, true, true);
   check(status_is("SAVING SLOT 1..."), "another game's task cannot finish this save");
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, true, false);
   check(status_is("SAVE FAILED"), "matching asynchronous failure reaches the document");
   click_and_frame(menu, "save");
   check(host.saves_started == 3 && status_is("SAVING SLOT 1..."),
         "asynchronous failure also permits a fresh attempt");
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, true, true);
   check(status_is("SLOT 1 SAVED"), "matching retry completion reports success");

   host.slot_occupied = true;
   frame(menu);
   host.load_accepted = false;
   click_and_frame(menu, "load");
   check(host.loads_started == 1 && status_is("LOAD FAILED"),
         "host load failure clears the transfer and reports failure");
   host.load_accepted = true;
   click_and_frame(menu, "load");
   check(host.loads_started == 2 && status_is("LOADING SLOT 1..."),
         "load retry waits for task completion");
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, false, true);
   check(status_is("SLOT 1 LOADED"), "matching load completion reports success");

   click_and_frame(menu, "options");
   char option_ids[16][64];
   const int option_count = view.document.focusables("options-panel", option_ids, 16);
   int sliders = 0;
   for (int index = 0; index < option_count; ++index) {
      auto *element = view.document.root()->GetElementById(option_ids[index]);
      check(!element->IsClassSet("volume-arrow"), "volume arrows are pointer-only targets");
      if (view.parts.part_is_slider(option_ids[index])) ++sliders;
   }
   check(sliders == 1, "volume has one logical keyboard/joypad stop");
   /* The slider is the first stop in the Options document, so we focus it
    * when the screen opens. Left and Right then move the level, not the focus. */
   check(focused(RIB_VOLUME_SLIDER_ID), "Options opens on the volume slider");
   {
      const float before = host.volume_db;
      rib_menu_key(menu, RIB_KEY_RIGHT);
      frame(menu);
      check(host.volume_db > before && focused(RIB_VOLUME_SLIDER_ID), "Right changes volume without leaving its control");
      rib_menu_key(menu, RIB_KEY_LEFT);
      frame(menu);
      check(std::fabs(host.volume_db - before) < 0.06f && focused(RIB_VOLUME_SLIDER_ID), "Left restores volume without an arrow focus stop");
   }
   check(view.parts.commit_slider(RIB_VOLUME_SLIDER_ID, 0.5f),
         "the staged Options screen exposes its volume slider");
   frame(menu);
   const std::string volume_path = std::string(argv[2]) + "/" + RIB_VOLUME_FILE;
   config_file_t *volume_file = config_file_new_from_path_to_string(volume_path.c_str());
   float saved_volume = 0.0f;
   check(volume_file && config_get_float(volume_file, RIB_VOLUME_KEY, &saved_volume)
         && std::fabs(saved_volume - host.volume_db) < 0.06f,
         "slider change persists through the real file/config layer");
   if (volume_file) config_file_free(volume_file);
   click_and_frame(menu, "controls");
   hover_and_frame(menu, "control-left");
   check(focused("control-left") && !focused("control-up"),
         "pointer focus paints only the hovered control");
   /* Down from LEFT is the callout drawn below it. */
   rib_menu_key(menu, RIB_KEY_DOWN);
   check(focused("control-down") && !focused("control-left"),
         "the next key continues from the control reached by pointer");

   hover_and_frame(menu, "control-up");
   host.capture_start_accepted = false;
   rib_menu_key(menu, RIB_KEY_OK);
   check(std::string(inspect.text("controls-status")) == "CAPTURE COULD NOT START",
         "a rejected keyboard capture does not fall through to Reset");
   host.capture_start_accepted = true;

   host.clock_us = 0;
   hover_and_frame(menu, "control-up");
   int popup_x = 0, popup_y = 0;
   check(!binds_visible(&popup_x, &popup_y),
         "bind list remains closed before the declared delay");
   host.clock_us = 1200000;
   frame(menu);
   check(binds_visible(&popup_x, &popup_y),
         "bind list opens after its declared delay");
   hover_and_frame(menu, "controls-reset");
   check(focused("controls-reset") && !focused("control-up"),
         "pointer on Reset moves the one focus away from a control");
   check(!binds_visible(&popup_x, &popup_y),
         "Reset hover closes the bind list");
   hover_and_frame(menu, "control-up");
   host.clock_us = 2400000;
   frame(menu);
   check(binds_visible(&popup_x, &popup_y),
         "bind list can reopen after Reset");
   host.pointer.x = popup_x;
   host.pointer.y = popup_y;
   frame(menu);
   check(focused("control-up")
         && binds_visible(&popup_x, &popup_y),
         "pointer over the popup retains its source control and the popup");
   host.pointer.x = 0;
   host.pointer.y = 0;
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
   rib_menu_key(menu, RIB_KEY_CANCEL);
   frame(menu);
   check(std::string(inspect.text("heading")) == "OPTIONS",
         "keyboard Back follows the declared destination through its listener");
   click_and_frame(menu, "fixture");
   view.document.set_shown("fixture-one", false);
   view.document.set_shown("fixture-two", false);
   rib_menu_key(menu, RIB_KEY_CANCEL);
   frame(menu);
   check(std::string(inspect.text("heading")) == "OPTIONS",
         "An empty account/list screen also follows its declared Back destination");
   view.document.set_shown("fixture-one", true);
   view.document.set_shown("fixture-two", true);
   click_and_frame(menu, "controls");
   host.pointer.x = 0;
   host.pointer.y = 0;
   host.pointer.pressed = true;
   click_and_frame(menu, "control-up");
   check(host.captures_started == 1, "control click starts capture through fake host");
   check(!host.capture_accepts_pointer, "the opening pointer gesture is excluded from capture");
   for (rib_key key : {RIB_KEY_UP, RIB_KEY_DOWN, RIB_KEY_LEFT, RIB_KEY_RIGHT,
         RIB_KEY_OK, RIB_KEY_SELECT, RIB_KEY_START})
   {
      rib_menu_key(menu, key);
      check(host.captures_started == 1 && host.captured_id == "up" && focused("control-up")
            && inspect.has_class("control-up", "capturing"),
            "capture keeps its target and ignores navigation, confirmation and Start");
   }
   host.capture_remaining = 5.0f;
   frame(menu);
   check(std::string(inspect.text("controls-status")) == "up: PRESS AN INPUT (5)",
         "capture countdown remains visible while the opening pointer is held");
   host.capture_remaining = 4.0f;
   frame(menu);
   check(std::string(inspect.text("controls-status")) == "up: PRESS AN INPUT (4)",
         "capture countdown continues while pointer input is gated");
   host.capture_result = RIB_CAPTURE_TIMED_OUT;
   host.capture_remaining = 0.0f;
   frame(menu);
   check(std::string(inspect.text("controls-status")) == "TIMED OUT; BINDING UNCHANGED",
         "host timeout ends capture even while the opening pointer is held");
   host.pointer.pressed = false;
   host.capture_result = RIB_CAPTURE_PENDING;
   host.capture_remaining = 9.0f;
   frame(menu);
   click_and_frame(menu, "control-up");
   check(host.captures_started == 2, "a fresh capture can start after timeout");
   frame(menu);
   check(host.capture_accepts_pointer, "pointer release enables capture input on the next frame");
   rib_menu_destroy(menu);
   check(host.captures_cancelled == 1, "destruction cancels live capture");
   check(!view.document.has_element("save"), "destruction releases the document");
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, true, true);

   menu = rib_menu_create();
   check(menu != nullptr, "create after destruction");
   if (menu)
   {
      frame(menu);
      check(view.document.has_element("save"), "new menu loads after prior destruction");
      rib_menu_toggle(menu, true);
      rib_menu_context_destroy(menu);
      check(!view.document.has_element("save"), "context destruction releases document");
      rib_menu_context_reset(menu);
      frame(menu);
      check(view.document.has_element("save"), "context reset reloads the document");
      rib_menu_destroy(menu);
   }

   if (failures)
      std::fprintf(stderr, "%d menu orchestration failures\n", failures);
   return failures ? 1 : 0;
}
