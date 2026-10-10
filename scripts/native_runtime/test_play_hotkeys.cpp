/* The hotkeys that act while the game plays, through the production menu,
 * its document and save slots, on a menu composed by an export: QUICK SAVE
 * and QUICK LOAD on the slot selected in the menu, PREVIOUS SLOT and NEXT
 * SLOT around the six slots, each once for each press and never while the
 * menu is open, and the notice for each in the notice row, and FULLSCREEN,
 * which the player also uses in the menu. In the fake host we stand in for
 * RetroArch. We set the keys held, report the state task and count each
 * switch to and from fullscreen.
 *
 *   test_play_hotkeys ASSETS DATA
 *
 * ASSETS is a composed Native menu with the builder's hotkeys. DATA is an
 * empty folder for this test. */
#include "rmlui/menu_api.h"
#include "rmlui/host.h"
#include "rmlui_bridge.h"
#include "rmlui/view.hpp"
#include "rmlui/elements.hpp"
#include "menu_test_view.hpp"
#include "menu_host_fake.h"
#include "test_arguments.h"
#include "test_environment.h"
#include <streams/file_stream.h>
#include <cstdio>
#include <string>
#include <vector>

namespace {
rib::View& view = rib::menu_view();
rib::test::Inspection inspect(view.document);
using rib::test::host;
int failures;

void check(bool condition, const std::string& message)
{
   if (!condition)
   {
      std::fprintf(stderr, "FAIL play hotkeys: %s\n", message.c_str());
      ++failures;
   }
}

void frame(void *menu) { rib::test::loop_pass(menu, 960, 600); }

/* One frame of the game with `keys` held, as in the RetroArch run loop: read
 * the hotkeys, then draw. */
void play(void *menu, std::vector<std::string> keys = {})
{
   host.keys_down = std::move(keys);
   rib_rmlui_play_hotkeys();
   frame(menu);
}

/* `key` pressed and let go while the game plays. */
void press(void *menu, const char *key)
{
   play(menu, {key});
   play(menu);
}

int selected() { return view.slots.selected(); }

/* The text in the notice row, when it has a notice about the slots. */
std::string notice()
{
   Rml::Element *row = view.document.root() ? view.document.root()->GetElementById("unlock-row") : nullptr;
   if (!row || rib::hidden(row))
      return "<no notice>";
   if (row->GetAttribute<Rml::String>("data-notice", "") != "slot")
      return "<a notice of another kind>";
   return inspect.words("unlock-title");
}

void expect_notice(const char *expected, const char *when)
{
   check(notice() == expected, std::string(when) + ": the notice says \"" + notice()
         + "\", expected \"" + expected + "\"");
}

void open_menu(void *menu, bool open)
{
   host.menu_open = open;
   rib_menu_toggle(menu, open);
   frame(menu);
}

/* Before the player has chosen a slot, the slot in the menu is the first,
 * and the hotkeys use that one. */
void the_first_slot_until_one_is_chosen(void *menu)
{
   check(selected() == 1, "the menu starts on slot 1, not " + std::to_string(selected()));
   expect_notice("<no notice>", "before any hotkey");
}

/* NEXT SLOT and PREVIOUS SLOT step through the slots in the menu, from the
 * last to the first and the reverse, and we show which slot is now chosen. */
void the_slot_steps_and_wraps(void *menu)
{
   press(menu, "f7");
   check(selected() == 2, "NEXT SLOT goes from 1 to 2, not " + std::to_string(selected()));
   expect_notice("SLOT 2", "NEXT SLOT");
   press(menu, "f6");
   press(menu, "f6");
   check(selected() == 6, "PREVIOUS SLOT from 1 goes to the last slot, not " + std::to_string(selected()));
   expect_notice("SLOT 6", "PREVIOUS SLOT from the first slot");
   press(menu, "f7");
   check(selected() == 1, "NEXT SLOT from the last goes to the first, not " + std::to_string(selected()));
   expect_notice("SLOT 1", "NEXT SLOT from the last slot");

   /* Held down, a key steps once. */
   for (int held = 0; held < 10; ++held)
      play(menu, {"f7"});
   play(menu);
   check(selected() == 2, "NEXT SLOT held for ten frames steps once, to 2, not " + std::to_string(selected()));
   press(menu, "f6");
}

/* QUICK LOAD on an empty slot loads nothing, and we show a notice. */
void an_empty_slot_is_not_loaded(void *menu)
{
   host.slot_occupied = false;
   const int loads = host.loads_started;
   press(menu, "f4");
   check(host.loads_started == loads, "QUICK LOAD of an empty slot starts no load");
   expect_notice("SLOT 1 IS EMPTY", "QUICK LOAD of an empty slot");
}

/* QUICK SAVE is the menu's SAVE on its slot: we set the RetroArch slot to
 * it and start a save, and once RetroArch reports the save, the notice shows
 * the slot. QUICK LOAD then loads it. */
void quick_save_and_load_use_the_menus_slot(void *menu)
{
   host.save_accepted = true;
   host.load_accepted = true;
   const int saves = host.saves_started;
   press(menu, "f2");
   check(host.saves_started == saves + 1, "QUICK SAVE starts one save");
   check(host.selected_slot == 1, "RetroArch saves to the menu's slot, 1, not " + std::to_string(host.selected_slot));
   check(view.slots.transfer_pending(), "the save is under way until RetroArch reports it");
   /* A second press while the save is under way starts nothing. */
   press(menu, "f2");
   check(host.saves_started == saves + 1, "QUICK SAVE while a save is under way starts no other");
   host.slot_occupied = true;
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, true, true);
   play(menu);
   expect_notice("SAVED TO SLOT 1", "a finished QUICK SAVE");
   check(view.slots.occupied(1), "the menu's slot 1 holds the save");

   const int loads = host.loads_started;
   press(menu, "f4");
   check(host.loads_started == loads + 1, "QUICK LOAD of a saved slot starts one load");
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, false, true);
   play(menu);
   expect_notice("LOADED SLOT 1", "a finished QUICK LOAD");

   /* When RetroArch rejects a save, we show a notice. */
   host.save_accepted = false;
   press(menu, "f2");
   play(menu);
   expect_notice("SAVE FAILED", "a QUICK SAVE RetroArch refused");
   host.save_accepted = true;
}

/* For a save from SAVE in the menu we show nothing in the notice row,
 * because the pause screen shows it. */
void the_menus_own_save_has_no_notice(void *menu)
{
   /* The last notice has expired. */
   host.clock_us += 5000000;
   play(menu);
   expect_notice("<no notice>", "five seconds after the last notice");
   open_menu(menu, true);
   view.document.click_element("save");
   frame(menu);
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, true, true);
   open_menu(menu, false);
   play(menu);
   expect_notice("<no notice>", "a save the menu's SAVE asked for");
}

/* While the menu is open nothing acts, and a key held as it closes acts only
 * once it is let go and pressed again. */
void nothing_acts_while_the_menu_is_open(void *menu)
{
   open_menu(menu, true);
   const int saves = host.saves_started;
   play(menu, {"f7"});
   play(menu, {"f2"});
   play(menu);
   check(selected() == 1 && host.saves_started == saves, "no hotkey acts while the menu is open");
   play(menu, {"f7"});
   host.keys_down = {"f7"};
   open_menu(menu, false);
   play(menu, {"f7"});
   play(menu, {"f7"});
   check(selected() == 1, "NEXT SLOT held as the menu closes does not act, on " + std::to_string(selected()));
   play(menu);
   press(menu, "f7");
   check(selected() == 2, "let go and pressed again, it steps to 2, not " + std::to_string(selected()));
}

/* The slot chosen with the hotkeys is the one shown as chosen in the menu,
 * and SAVE and LOAD use it. */
void the_menu_shows_the_chosen_slot(void *menu)
{
   press(menu, "f7");
   open_menu(menu, true);
   check(inspect.has_class("slot-3", "selected") && !inspect.has_class("slot-1", "selected"),
         "the pause screen shows slot 3 chosen");
   const int saves = host.saves_started;
   view.document.click_element("save");
   frame(menu);
   check(host.saves_started == saves + 1 && host.selected_slot == 3,
         "SAVE saves to slot 3, the slot NEXT SLOT chose, not " + std::to_string(host.selected_slot));
   rib_rmlui_notify_state_task("", 3, true, true);
   frame(menu);
   open_menu(menu, false);
}

/* With FULLSCREEN, which the player bound to F11, the player switches between
 * fullscreen and a window once for each press, during play and in the menu.
 * While the player chooses a binding, we give the press to the capture. */
void fullscreen_acts_in_play_and_in_the_menu(void *menu)
{
   const int before = host.fullscreen_toggles;
   press(menu, "f11");
   check(host.fullscreen_toggles == before + 1, "F11 during play switches to fullscreen");
   for (int held = 0; held < 10; ++held)
      play(menu, {"f11"});
   play(menu);
   check(host.fullscreen_toggles == before + 2, "F11 held for ten frames switches once");
   open_menu(menu, true);
   press(menu, "f11");
   check(host.fullscreen_toggles == before + 3, "F11 in the menu switches too");

   const int captures = host.input_captures_started;
   for (const char *id : {"options", "hotkeys", "hotkey-quick-save-add"})
   {
      check(view.document.click_element(id), std::string("click ") + id);
      frame(menu);
   }
   check(host.input_captures_started == captures + 1, "+ on QUICK SAVE starts a capture");
   press(menu, "f11");
   check(host.fullscreen_toggles == before + 3,
         "F11 pressed while the player chooses a binding does not switch");
   check(view.document.click_element("hotkeys-cancel"), "click CANCEL");
   frame(menu);
   open_menu(menu, false);
}

/* The next launch starts on the slot chosen last, and QUICK SAVE saves to
 * it, as before the game was closed. */
void the_next_launch_keeps_the_chosen_slot(void *menu)
{
   check(selected() == 3, "the next launch starts on slot 3, the slot chosen last, not "
         + std::to_string(selected()));
   const int saves = host.saves_started;
   press(menu, "f2");
   check(host.saves_started == saves + 1 && host.selected_slot == 3,
         "QUICK SAVE on the next launch saves to slot 3, not " + std::to_string(host.selected_slot));
   rib_rmlui_notify_state_task("", 3, true, true);
   play(menu);
}
}

/* With its window in the background, and PLAY IN BACKGROUND off, the game
 * waits, paused. We say so on the body for the design, and draw over the
 * game while it waits, also on a document built again meanwhile, as after a
 * switch to fullscreen. With the window in front again, the game plays. */
void the_game_waiting_in_the_background_is_drawn_over(void *menu)
{
   const auto says = [] { return view.document.root()->IsClassSet("paused-in-background"); };
   play(menu);
   check(!says(), "a game that plays is not said to wait");
   host.waits_in_background = true;
   play(menu);
   check(says(), "the body says the game waits in the background");
   check(rib_rmlui_overlays_drawing() && host.overlay_frames, "we draw over the game while it waits");
   rib_menu_context_destroy(menu);
   rib_menu_context_reset(menu);
   frame(menu);
   check(says(), "a document built again while the game waits says so");
   host.waits_in_background = false;
   play(menu);
   check(!says(), "with the window in front again, the body no longer says so");
}

int main(int argc, char **argv)
{
   Utf8Arguments utf8(argc, argv);
   argc = utf8.argc();
   argv = utf8.argv();
   if (argc != 3)
   {
      std::fprintf(stderr, "usage: %s ASSETS DATA\n", argv[0]);
      return 2;
   }
   test_setenv("ROMINABOX_RML_ASSETS", argv[1]);
   test_setenv("ROMINABOX_DATA_DIR", argv[2]);
   test_unsetenv("ROMINABOX_MENU_SCRIPT");
   host.clock_us = 1000000;
   host.menu_open = false;
   /* FULLSCREEN has no binding by default. This player bound F11 to it. */
   const std::string bound = "hotkey_fullscreen = \"key:f11\"\n";
   filestream_write_file((std::string(argv[2]) + "/hotkeys.cfg").c_str(), bound.data(),
         (int64_t)bound.size());
   void *menu = rib_menu_create();
   check(menu != nullptr, "the menu is made");
   if (!menu)
      return 1;
   /* The game plays with the menu closed, and we draw the document over it. */
   frame(menu);
   play(menu);
   check(view.document.root() != nullptr, "the menu's document loads while the game plays");

   the_first_slot_until_one_is_chosen(menu);
   the_slot_steps_and_wraps(menu);
   an_empty_slot_is_not_loaded(menu);
   quick_save_and_load_use_the_menus_slot(menu);
   the_menus_own_save_has_no_notice(menu);
   nothing_acts_while_the_menu_is_open(menu);
   the_menu_shows_the_chosen_slot(menu);
   fullscreen_acts_in_play_and_in_the_menu(menu);
   the_game_waiting_in_the_background_is_drawn_over(menu);
   rib_menu_destroy(menu);

   /* The game closed and opened again, on the same data. */
   menu = rib_menu_create();
   check(menu != nullptr, "the menu is made again");
   if (!menu)
      return 1;
   frame(menu);
   play(menu);
   the_next_launch_keeps_the_chosen_slot(menu);
   rib_menu_destroy(menu);

   if (failures)
      std::fprintf(stderr, "play hotkeys: %d failures\n", failures);
   else
      std::printf("play hotkeys: every case passed\n");
   return failures ? 1 : 0;
}
