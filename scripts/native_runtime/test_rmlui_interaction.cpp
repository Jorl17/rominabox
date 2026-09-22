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

/* Two controllers, so there is a choice in the picker, as on the Mega Drive. */
static const char *stub_device_ids[] = {"megadrive", "megadrive6"};
static const char *stub_device_names[] = {"Mega Drive", "Mega Drive six-button"};

extern "C" int rib_rmlui_device_count(void) { return 2; }
extern "C" const char *rib_rmlui_device_id(int index)
{
   return (index >= 0 && index < 2) ? stub_device_ids[index] : nullptr;
}
extern "C" const char *rib_rmlui_device_name(int index)
{
   return (index >= 0 && index < 2) ? stub_device_names[index] : nullptr;
}

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

// Press on one element and release somewhere else. People do this: they put
// the button down, change their mind, slide off and let go. Nothing should
// happen.
static void press_then_release_at(const char *id, int x, int y)
{
   int from_x = 0;
   int from_y = 0;
   CHECK(rib_rmlui_element_center(id, &from_x, &from_y), "element has a hit centre");
   rib_rmlui_pointer_move(from_x, from_y);
   rib_rmlui_pointer_button(true);
   rib_rmlui_pointer_move(x, y);
   rib_rmlui_pointer_button(false);
}

static void drain_actions(void)
{
   while (rib_rmlui_take_action() != RIB_RMLUI_ACTION_NONE)
      ;
}

/* Draw for this long. RmlUi advances an animation by at most a tenth of a
 * second per update, so a transition finishes only if we draw frames while
 * the clock moves, as at sixty frames a second in a running game. */
static void settle(double seconds)
{
   for (double at = 0; at < seconds; at += 0.05)
   {
      rib_rmlui_test_advance(0.05);
      rib_rmlui_render(960, 600);
   }
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
   /* The screen button on the pause row is Options. `controls` is inside
    * that panel, so this click cannot reach the built-in handler for
    * `controls`. */
   rib_rmlui_declare_screen("options", "options-panel", "OPTIONS",
         "ESC  BACK", "options");

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
   click_id("options");
   const int first = rib_rmlui_take_action();
   const int second = rib_rmlui_take_action();
   CHECK(first == RIB_RMLUI_ACTION_SAVE,
         "mailbox preserves the first click");
   // Changing screen has no separate action. We pass the requested screen
   // next to one shared action, so declaring a screen never adds to the
   // enum. This test is mainly about the order, and it also checks that the
   // id arrived.
   CHECK(second == RIB_RMLUI_ACTION_SHOW_SCREEN,
         "mailbox preserves the following click");
   CHECK(std::string(rib_rmlui_requested_screen()) == "options",
         "the screen asked for travels with the action");
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

   /* We write the controls scene into menu.rml in the builder. This template
    * still has the placeholder, and nothing in the bridge replaces it. We
    * write the remap in rmlui.c, which is not linked here, so this does not
    * prove that a choice is saved or applied. */
   rib_rmlui_show_controls(true);
   {
      int image_x = 0;
      int image_y = 0;
      const std::string scene(rib_rmlui_test_text("controller-scene"));
      CHECK(scene.find("control-") == std::string::npos,
            "the player template has no generated control callouts");
      CHECK(!rib_rmlui_element_center("controller-image", &image_x, &image_y),
            "the player template has no controller illustration");
      rib_rmlui_wire_device_picker();
      rib_rmlui_set_device_picker(true, "megadrive6");
      CHECK(std::string(rib_rmlui_test_text("controller-scene")) == scene,
            "naming another pad does not redraw the controls scene");
      CHECK(!rib_rmlui_element_center("controller-image", &image_x, &image_y),
            "naming another pad does not add an illustration");
      CHECK(!rib_rmlui_element_center(
            "controls-device-option-megadrive6", &image_x, &image_y),
            "picker options are export markup, not created by the bridge");
   }

   CHECK(RIB_VOLUME_POSITIONS > 1 && RIB_VOLUME_POSITIONS < 10,
         "volume has a handful of positions, fewer than ten");
   CHECK(AUDIO_VOLUME_MAX_DB == 0.0f,
         "the right end is normal, and the control cannot boost past it");
   CHECK(AUDIO_VOLUME_DEFAULT_DB == AUDIO_VOLUME_MAX_DB,
         "the default is the maximum");
   CHECK(AUDIO_VOLUME_STEP_DB * (RIB_VOLUME_POSITIONS - 1)
               == AUDIO_VOLUME_MAX_DB - AUDIO_VOLUME_MIN_DB,
         "the positions are equal steps from quiet to normal");
   CHECK(rib_volume_db_from_fraction(0.0f) == AUDIO_VOLUME_MIN_DB,
         "the left end of the slider is the quietest it goes");
   CHECK(rib_volume_db_from_fraction(1.0f) == AUDIO_VOLUME_MAX_DB,
         "the right end of the slider is normal");
   CHECK(rib_volume_db_from_fraction(rib_volume_fraction_from_db(0.0f)) == 0.0f,
         "normal, the default, round-trips through the slider");
   CHECK(rib_volume_db_from_fraction(-1.0f) == AUDIO_VOLUME_MIN_DB,
         "a drag past the left end stops at the end");
   CHECK(rib_volume_db_from_fraction(2.0f) == AUDIO_VOLUME_MAX_DB,
         "a drag past the right end stops at normal");
   CHECK(rib_volume_quantize_db(-6.4f) == 0.0f,
         "a level near the top snaps to a position, not to the nearest decibel");

   /* We do not read design.cfg in the interaction harness. We declare Options
    * as an export writes it, so showing it shows the screen that a player
    * opens. */
   rib_rmlui_clear_screens();
   rib_rmlui_declare_screen("pause", "pause-panel", "GAME PAUSED",
         "ESC  CONTINUE", "options-back");
   rib_rmlui_declare_screen("options", "options-panel", "OPTIONS",
         "ESC  BACK", "options");
   rib_rmlui_declare_screen("controls", "controls-panel", "CONTROLS",
         "ESC  BACK", "controls");
   rib_rmlui_show_screen("options");
   {
      int slider_x = 0;
      int slider_y = 0;
      int mute_x = 0;
      int mute_y = 0;
      CHECK(rib_rmlui_element_center("volume-level", &slider_x, &slider_y),
            "Options has the design's slider");
      CHECK(!rib_rmlui_element_center("volume-mute", &mute_x, &mute_y),
            "there is no mute button");
      rib_rmlui_clear_intents();
      rib_rmlui_pointer_move(slider_x, slider_y);
      rib_rmlui_pointer_button(true);
      rib_rmlui_pointer_move(0, 0);
      rib_rmlui_pointer_button(false);
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SLIDER,
            "dragging off a slider still sets the level");
      CHECK(std::string(rib_rmlui_changed_part()) == "volume-level",
            "the slider reports which part moved");
      CHECK(rib_rmlui_changed_fraction() == 0.0f,
            "a drag off the left end is the bottom of the range");

      rib_rmlui_set_slider("volume-level", 0.5f, nullptr);
      rib_rmlui_clear_intents();
      CHECK(!rib_rmlui_nudge_slider("volume-level", 1),
            "a slider with no step does not move, so a key cannot invent one");
      rib_rmlui_set_slider_step("volume-level", 0.1f);
      CHECK(rib_rmlui_nudge_slider("volume-level", 1), "a key nudges the focused slider");
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SLIDER,
            "the nudge is the same change a drag commits");
      CHECK(rib_rmlui_changed_fraction() > 0.59f && rib_rmlui_changed_fraction() < 0.61f,
            "the nudge adds the slider's own step, not a volume-shaped one");

      rib_rmlui_set_slider("volume-level", 1.0f, nullptr);
      rib_rmlui_set_slider_step("volume-level",
            AUDIO_VOLUME_STEP_DB / (AUDIO_VOLUME_MAX_DB - AUDIO_VOLUME_MIN_DB));
      rib_rmlui_clear_intents();
      click_id("volume-down");
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SLIDER,
            "the left arrow is the slider moving down one position");
      CHECK(rib_rmlui_changed_fraction() > 0.74f && rib_rmlui_changed_fraction() < 0.76f,
            "one arrow is one position, not a decibel");
   }

   // Letting go somewhere else must not press the button.
   //
   // In the recorded interaction scenario we press and release at the same
   // point, which is the easy half. This test covers the other half.
   {
      // Earlier checks can leave the controls screen up, where SAVE is hidden
      // and nothing can be clicked, so we set the screen explicitly.
      rib_rmlui_show_screen("pause");
      drain_actions();
      press_then_release_at("save", 4, 4);
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
            "pressing a button and releasing off it does nothing");

      // The other half, to show that this does not pass because clicks have
      // stopped working: a press and release on the same button still acts.
      drain_actions();
      click_id("save");
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SAVE,
            "pressing and releasing on a button still presses it");

      // Sliding off and back on is a press, because the release happens on the
      // element where the press began.
      drain_actions();
      {
         int x = 0;
         int y = 0;
         CHECK(rib_rmlui_element_center("save", &x, &y), "element has a hit centre");
         rib_rmlui_pointer_move(x, y);
         rib_rmlui_pointer_button(true);
         rib_rmlui_pointer_move(4, 4);
         rib_rmlui_pointer_move(x, y);
         rib_rmlui_pointer_button(false);
      }
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SAVE,
            "sliding off a button and back on still presses it");
      drain_actions();
   }

   // Every intent that we can queue in the menu plays a sound, unless we chose
   // silence for it on purpose, so an action added later cannot be silent
   // without a test failure. Changing screen, for example, must still play
   // the confirm cue of the menu.
   {
      const int silent[] = {
         RIB_RMLUI_ACTION_NONE,
         RIB_RMLUI_ACTION_SELECT_SLOT_1, RIB_RMLUI_ACTION_SELECT_SLOT_2,
         RIB_RMLUI_ACTION_SELECT_SLOT_3, RIB_RMLUI_ACTION_SELECT_SLOT_4,
         RIB_RMLUI_ACTION_SELECT_SLOT_5, RIB_RMLUI_ACTION_SELECT_SLOT_6,
      };
      for (int action = RIB_RMLUI_ACTION_NONE;
            action <= RIB_RMLUI_ACTION_SHOW_SCREEN; ++action)
      {
         bool expected_silent = false;
         for (int quiet : silent)
            if (quiet == action)
               expected_silent = true;
         const bool is_silent =
            rib_rmlui_action_sound(action) == RIB_MENU_SOUND_NONE;
         CHECK(is_silent == expected_silent,
               "every intent is audible unless silence was chosen for it");
      }
      CHECK(rib_rmlui_action_sound(RIB_RMLUI_ACTION_SHOW_SCREEN)
            == RIB_MENU_SOUND_OK, "changing screen is confirmed, not silent");
      CHECK(rib_rmlui_action_sound(RIB_RMLUI_ACTION_CONTROLS_BACK)
            == RIB_MENU_SOUND_CANCEL, "leaving a screen cancels, not confirms");
   }

   // We draw an overlay over a running game, and how it looks is up to the
   // design. In the player we only move an element between three states and
   // state whether the menu is on screen. If the rules in the design did not
   // act on that, the notice would appear and vanish without the arriving,
   // leaving or hiding animations in the design.
   {
      rib_rmlui_set_overlay("notice", RIB_OVERLAY_HIDDEN);
      rib_rmlui_set_overlay_mode(true);
      rib_rmlui_render(960, 600);
      const std::string away = rib_rmlui_test_property("notice", "opacity");
      const std::string resting = rib_rmlui_test_property("notice", "bottom");
      CHECK(std::string(rib_rmlui_test_property("footer", "display")) == "none",
            "the design puts the menu away while only overlays are drawn");

      rib_rmlui_set_overlay("notice", RIB_OVERLAY_SHOWING);
      settle(0.6);
      const std::string shown = rib_rmlui_test_property("notice", "opacity");
      CHECK(shown != away, "showing an overlay makes the design draw it");
      CHECK(std::string(rib_rmlui_test_property("notice", "bottom")) != resting,
            "the design moves the notice into place, not only fades it in");

      rib_rmlui_set_overlay("notice", RIB_OVERLAY_LEAVING);
      rib_rmlui_render(960, 600);
      CHECK(std::string(rib_rmlui_test_property("notice", "opacity")) != away,
            "leaving is a transition, not a cut: the first frame is still drawn");
      settle(1.0);
      CHECK(std::string(rib_rmlui_test_property("notice", "opacity")) == away,
            "the design takes the notice away over its own declared time");

      rib_rmlui_set_overlay("notice", RIB_OVERLAY_HIDDEN);
      rib_rmlui_set_overlay_mode(false);
      rib_rmlui_render(960, 600);
      CHECK(std::string(rib_rmlui_test_property("footer", "display")) != "none",
            "the menu comes back when it is what is on screen");
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
