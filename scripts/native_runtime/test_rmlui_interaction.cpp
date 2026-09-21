/* Headless regression checks against the actual RmlUi bridge and domain helpers.
 * We compile rmlui_bridge.cpp with a dummy renderer and create no window. */

#include "rmlui_bridge.h"

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
/* rmlui.c is not linked here because it depends on the whole of RetroArch,
 * so we stub the control list that it normally supplies. With the stub a test
 * can declare more than sixteen controls, and the bridge must address all of
 * them. A PlayStation DualShock declares twenty-four. */
static const char *stub_control_ids[] = {
   "up", "down", "left", "right", "a", "b", "x", "y",
   "l", "r", "l2", "r2", "l3", "r3", "start", "select",
   "l_x_plus", "l_x_minus", "l_y_plus", "l_y_minus",
   "r_x_plus", "r_x_minus", "r_y_plus", "r_y_minus"
};
static const int stub_control_count =
   (int)(sizeof(stub_control_ids) / sizeof(stub_control_ids[0]));

extern "C" int rib_rmlui_control_capacity(void) { return stub_control_count; }
extern "C" const char *rib_rmlui_control_id(int index)
{
   if (index < 0 || index >= stub_control_count)
      return nullptr;
   return stub_control_ids[index];
}

extern "C" unsigned rib_rmlui_test_texture_loads();
extern "C" const char *rib_rmlui_test_property(const char *, const char *);

extern "C" void rib_rmlui_test_advance(double);
extern "C" const char *rib_rmlui_test_text(const char *);
extern "C" float rib_rmlui_test_picture_aspect();
static int failures = 0;

#define CHECK(cond, msg) \
   do { \
      if (!(cond)) { \
         std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, msg); \
         ++failures; \
      } \
   } while (0)

static void click_id(const char *id)
{
   int x = 0;
   int y = 0;
   CHECK(rib_rmlui_element_center(id, &x, &y), "element has a hit centre");
   rib_rmlui_pointer_move(x, y);
   rib_rmlui_pointer_button(true);
   rib_rmlui_pointer_button(false);
}

static void move_to_id(const char *id)
{
   int x = 0;
   int y = 0;
   CHECK(rib_rmlui_element_center(id, &x, &y), "element has a hover centre");
   rib_rmlui_pointer_move(x, y);
}

int main(int argc, char **argv)
{
   const char *assets = argc > 1 ? argv[1] : nullptr;
   if (!assets || !*assets)
   {
      std::fprintf(stderr, "usage: test_rmlui_interaction ASSET_DIR\n");
      return 2;
   }

   CHECK(rib_rmlui_map_menu_toggle(false, true) ==
            RIB_RMLUI_ACTION_CONTROLS_CANCEL,
         "toggle cancels capture first");
   CHECK(rib_rmlui_map_menu_toggle(true, false) ==
            RIB_RMLUI_ACTION_CONTROLS_BACK,
         "toggle leaves Controls next");
   CHECK(rib_rmlui_map_menu_toggle(false, false) ==
            RIB_RMLUI_ACTION_RESUME,
         "toggle resumes from the main screen");
   CHECK(rib_rmlui_toggle_stays_in_menu(true, false),
         "Controls keeps the menu open");
   CHECK(!rib_rmlui_ok_includes_pointer_select(true),
         "RmlUi OK does not consume the pointer select bit");
   CHECK(!rib_rmlui_load_is_actionable(false),
         "empty Load is not actionable");
   CHECK(!rib_rmlui_state_task_matches(false, true, "/s", 1, "/s", 1, true),
         "no pending operation does not match");
   CHECK(!rib_rmlui_state_task_matches(true, true, "/s", 1, "/s", 1, false),
         "a load result does not resolve a save");
   CHECK(!rib_rmlui_state_task_matches(true, true, "/s1", 1, "/s2", 1, true),
         "another path is ignored");
   CHECK(!rib_rmlui_state_task_matches(true, true, "/s", 1, "/s", 2, true),
         "another slot is ignored");
   CHECK(rib_rmlui_state_task_matches(true, false, "/s", 3, "/s", 3, false),
         "exact load path and slot match");

   if (!rib_rmlui_init(assets, 960, 600))
   {
      std::fprintf(stderr, "FAIL could not init RmlUi from %s\n", assets);
      return 1;
   }

   rib_rmlui_set_status("SAVED");
   rib_rmlui_set_controls_status("DEFAULTS RESTORED");
   rib_rmlui_test_advance(4);
   rib_rmlui_render(960, 600);
   CHECK(std::string(rib_rmlui_test_text("status")) == "SAVED", "status remains briefly");
   rib_rmlui_test_advance(2);
   rib_rmlui_render(960, 600);
   CHECK(std::string(rib_rmlui_test_text("status")).empty(), "main status expires");
   CHECK(std::string(rib_rmlui_test_text("controls-status")).empty(), "controls status expires");
   for (float aspect : {10.0f/9, 4.0f/3, 16.0f/9}) {
      rib_rmlui_set_game_aspect(aspect);
      CHECK(std::abs(rib_rmlui_test_picture_aspect() - aspect) < 0.02f, "well follows live core aspect");
   }
   rib_rmlui_set_game_aspect(4.0f/3);
   click_id("save");
   click_id("controls");
   const int first = rib_rmlui_take_action();
   const int second = rib_rmlui_take_action();
   CHECK(first == RIB_RMLUI_ACTION_SAVE,
         "mailbox preserves the first click");
   CHECK(second == RIB_RMLUI_ACTION_CONTROLS,
         "mailbox preserves the following click");
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
         "mailbox is empty after both intents");

   move_to_id("resume");
   CHECK(rib_rmlui_hovered_action() == RIB_RMLUI_ACTION_RESUME,
         "pointer hover tracks Resume");
   rib_rmlui_pointer_move(8, 8);
   CHECK(rib_rmlui_hovered_action() == RIB_RMLUI_ACTION_NONE,
         "pointer leave clears hover instead of sticking");

   rib_rmlui_set_focused(RIB_RMLUI_ACTION_QUIT);
   rib_rmlui_set_selected_slot(4);
   move_to_id("resume");
   CHECK(rib_rmlui_hovered_action() == RIB_RMLUI_ACTION_RESUME,
         "hover is independent of keyboard focus");

   rib_rmlui_set_selected_slot(4);
   rib_rmlui_set_focused(RIB_RMLUI_ACTION_RESUME);
   rib_rmlui_pointer_move(1, 1);
   const std::string selected_border = rib_rmlui_test_property("slot-4", "border-top-color");
   move_to_id("slot-4");
   rib_rmlui_set_focused(RIB_RMLUI_ACTION_SELECT_SLOT_1 + 3);
   CHECK(selected_border == rib_rmlui_test_property("slot-4", "border-top-color"),
         "selected slot keeps its border across hover and keyboard focus");
   rib_rmlui_pointer_button(true);
   CHECK(selected_border != rib_rmlui_test_property("slot-4", "border-top-color"),
         "slot has pressed feedback while held");
   rib_rmlui_pointer_move(1, 1);
   rib_rmlui_pointer_button(false);

   rib_rmlui_set_slot_state(1, false, nullptr);
   CHECK(rib_rmlui_element_disabled("load"),
         "empty Load is disabled");
   rib_rmlui_clear_intents();
   click_id("load");
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
         "disabled Load does not enqueue an action");

   rib_rmlui_show_controls(true);
   int control_x = 0, control_y = 0;
   if (rib_rmlui_element_center("control-up", &control_x, &control_y)) {
      rib_rmlui_set_control_state("up", "Up", "up", true, true);
      const std::string animation = rib_rmlui_test_property("control-up", "animation");
      CHECK(animation.find("capture-pulse") != std::string::npos, "capture animates the control itself");
      rib_rmlui_render(960, 600);
      const std::string border = rib_rmlui_test_property("control-up", "border-top-color");
      rib_rmlui_test_advance(0.3);
      rib_rmlui_render(960, 600);
      CHECK(border != rib_rmlui_test_property("control-up", "border-top-color"), "capture border changes over time");
      rib_rmlui_set_control_state("up", "Up", "up", true, false);
      CHECK(std::string(rib_rmlui_test_property("control-up", "animation")).find("capture-pulse") == std::string::npos, "capture cue stops when capture ends");
   }
   rib_rmlui_set_controls_action_focus(false, false, true);
   int cancel_x = 0;
   int cancel_y = 0;
   CHECK(rib_rmlui_element_center("controls-cancel", &cancel_x, &cancel_y),
         "Cancel has a hit centre while capture is visible");
   rib_rmlui_clear_intents();
   rib_rmlui_pointer_move(cancel_x, cancel_y);
   rib_rmlui_pointer_button(true);
   rib_rmlui_pointer_button(false);
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_CONTROLS_CANCEL,
         "Cancel is consumed by RmlUi before any binder poll");

   for (const char *id : {"controls-cancel", "controls-reset", "controls-back"})
   {
      move_to_id(id);
      const std::string hovered = rib_rmlui_test_property(id, "border-top-color");
      rib_rmlui_pointer_button(true);
      CHECK(hovered != rib_rmlui_test_property(id, "border-top-color"),
            "press is visible while pointer remains over a controls button");
      rib_rmlui_pointer_move(1, 1);
      rib_rmlui_pointer_button(false);
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
            "dragging out and releasing does not activate a controls button");
   }

   rib_rmlui_show_controls(false);
   rib_rmlui_clear_intents();
   rib_rmlui_pointer_button(true);
   rib_rmlui_pointer_leave();
   click_id("quit");
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_QUIT,
         "pointer down/up stay in sync after leave");

   rib_rmlui_clear_intents();
   rib_rmlui_show_controls(true);
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
         "screen transition drops stale mailbox intents");

   if (argc > 2)
   {
      rib_rmlui_show_controls(false);
      FILE *image = std::fopen(argv[2], "wb");
      CHECK(image, "writable thumbnail fixture");
      if (image) { std::fputs("first", image); std::fclose(image); }
      rib_rmlui_set_slot_state(2, true, argv[2]);
      rib_rmlui_render(960, 600);
      const unsigned before = rib_rmlui_test_texture_loads();
      image = std::fopen(argv[2], "wb");
      if (image) { std::fputs("updated image content", image); std::fclose(image); }
      rib_rmlui_set_slot_state(2, true, argv[2]);
      rib_rmlui_render(960, 600);
      CHECK(rib_rmlui_test_texture_loads() > before,
            "overwriting a thumbnail reloads the same file without reopening the menu");
   }

   rib_rmlui_shutdown();
   if (failures)
   {
      std::fprintf(stderr, "%d check(s) failed\n", failures);
      return 1;
   }
   std::printf("ok\n");
   return 0;
}
