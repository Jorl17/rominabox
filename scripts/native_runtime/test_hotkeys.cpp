/* HOTKEYS through the production menu, with its document and file layer, on
 * a menu composed by an export: what we show on the screen on each page,
 * adding a binding with the same capture as in Controls, removing one, the
 * rules that nobody can break, the swap, RESET, what survives a relaunch and
 * a later export, and what the menu hotkeys become in RetroArch input. Only
 * the RetroArch host commands are fake. test_play_hotkeys covers what the
 * hotkeys that act while the game plays do.
 *
 *   test_hotkeys ASSETS OTHER_ASSETS DATA
 *
 * ASSETS is a composed Native menu with the builder's defaults. OTHER_ASSETS
 * is the same menu exported again with other defaults (CONFIRM Space and the
 * top button). DATA is an empty folder for this test. */
#include <algorithm>
#include "rmlui/menu_api.h"
#include "rmlui/host.h"
#include "rmlui_bridge.h"
#include "rmlui/view.hpp"
#include "menu_test_view.hpp"
#include "menu_host_fake.h"
#include "test_arguments.h"
#include "test_environment.h"
#include <file/file_path.h>
#include <streams/file_stream.h>
#include <libretro.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

namespace {
rib::View& view = rib::menu_view();
rib::test::Inspection inspect(view.document);
using rib::test::host;
int failures;
std::string data;

void check(bool condition, const std::string& message)
{
   if (!condition)
   {
      std::fprintf(stderr, "FAIL hotkeys: %s\n", message.c_str());
      ++failures;
   }
}

void frame(void *menu) { rib::test::loop_pass(menu, 960, 600); }

void click(void *menu, const char *id)
{
   check(view.document.click_element(id), std::string("click ") + id);
   frame(menu);
}

void *open(const char *assets)
{
   test_setenv("ROMINABOX_RML_ASSETS", assets);
   void *menu = rib_menu_create();
   check(menu != nullptr, "the menu is made");
   if (!menu)
      return nullptr;
   frame(menu);
   rib_menu_toggle(menu, true);
   frame(menu);
   click(menu, "options");
   click(menu, "hotkeys");
   return menu;
}

void close(void *menu)
{
   rib_menu_toggle(menu, false);
   rib_menu_destroy(menu);
}

/* The chips of a row as we show them in the menu, in order: the words of
 * each visible chip, on whichever page the row is. */
std::vector<std::string> row(const char *hotkey)
{
   std::vector<std::string> shown;
   for (int chip = 1; chip <= 5; ++chip)
   {
      const std::string id = std::string("hotkey-") + hotkey + "-" + std::to_string(chip);
      Rml::Element *element = view.document.root()->GetElementById(id);
      if (!element || rib::display_none(element))
         continue;
      /* The words on a chip, besides anything else the design draws in it. */
      Rml::Element *words = rib::find_class(element, "hotkey-words");
      shown.push_back(words ? words->GetInnerRML() : inspect.text(id.c_str()));
   }
   return shown;
}

std::string joined(const std::vector<std::string>& words)
{
   std::string text;
   for (const std::string& word : words)
      text += (text.empty() ? "" : ", ") + word;
   return text;
}

void expect_row(const char *hotkey, const std::vector<std::string>& expected, const char *when)
{
   const std::vector<std::string> shown = row(hotkey);
   check(shown == expected, std::string(when) + ": " + hotkey + " shows [" + joined(shown)
         + "], expected [" + joined(expected) + "]");
}

std::string status() { return inspect.text("hotkeys-status"); }

void expect_status(const char *expected, const char *when)
{
   check(status() == expected, std::string(when) + ": the status says \"" + status()
         + "\", expected \"" + expected + "\"");
}

/* The capture of + on `hotkey`, answered with `input`. */
void capture(void *menu, const char *hotkey, const char *input)
{
   const int before = host.input_captures_started;
   click(menu, (std::string("hotkey-") + hotkey + "-add").c_str());
   check(host.input_captures_started == before + 1,
         std::string("+ on ") + hotkey + " starts a capture");
   host.captured_input = input;
   host.capture_result = RIB_CAPTURE_CAPTURED;
   frame(menu);
   host.capture_result = RIB_CAPTURE_PENDING;
}

std::string player_file()
{
   void *bytes = nullptr;
   int64_t size = 0;
   const std::string path = data + "/hotkeys.cfg";
   if (!filestream_read_file(path.c_str(), &bytes, &size))
      return std::string();
   std::string text(static_cast<const char*>(bytes), (size_t)size);
   free(bytes);
   return text;
}

/* What we read from RetroArch input with `keys` and `pads` held: the RetroPad
 * buttons of the menu, as the runloop passes them to the menu with RetroPad A
 * as OK and B as cancel, and whether MENU is held. */
struct Pressed
{
   bool ok, cancel, menu_pad, start;
   std::vector<unsigned> menu_keys;
};

Pressed press(std::vector<std::string> keys, std::vector<std::string> pads, uint32_t buttons = 0)
{
   host.keys_down = std::move(keys);
   host.pads_down = std::move(pads);
   rib_rmlui_menu_buttons(&buttons, RETRO_DEVICE_ID_JOYPAD_A, RETRO_DEVICE_ID_JOYPAD_B);
   Pressed pressed;
   pressed.ok = buttons & (1u << RETRO_DEVICE_ID_JOYPAD_A);
   pressed.cancel = buttons & (1u << RETRO_DEVICE_ID_JOYPAD_B);
   pressed.start = buttons & (1u << RETRO_DEVICE_ID_JOYPAD_START);
   pressed.menu_pad = rib_rmlui_menu_pad_held();
   unsigned codes[8];
   pressed.menu_keys.assign(codes, codes + rib_rmlui_menu_keys(codes, 8));
   host.keys_down.clear();
   host.pads_down.clear();
   return pressed;
}

unsigned key_code(const char *name)
{
   unsigned code = 0;
   rib_host_key_code(name, &code);
   return code;
}

/* What we do in the menu with a RetroPad A or B from the runloop. In the menu
 * driver we turn MENU_ACTION_OK and MENU_ACTION_CANCEL into menu keys. */
void act(void *menu, const Pressed& pressed)
{
   if (pressed.ok)
      rib_menu_key(menu, RIB_KEY_OK);
   if (pressed.cancel)
      rib_menu_key(menu, RIB_KEY_CANCEL);
   frame(menu);
}

void defaults_and_words(void *menu)
{
   check(view.document.has_element("hotkeys"), "Options offers HOTKEYS");
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "the defaults");
   expect_row("confirm", {"Enter", "Bottom button"}, "the defaults");
   expect_row("back", {"Escape", "Right button"}, "the defaults");
   expect_row("quick-save", {"f2"}, "the defaults");
   expect_row("quick-load", {"f4"}, "the defaults");
   expect_row("previous-slot", {"f6"}, "the defaults");
   expect_row("next-slot", {"f7"}, "the defaults");
   check(inspect.has_class("hotkey-menu-1", "key") && !inspect.has_class("hotkey-menu-1", "pad"),
         "a key's chip is marked key");
   check(inspect.has_class("hotkey-menu-2", "pad") && !inspect.has_class("hotkey-menu-2", "key"),
         "a pad input's chip is marked pad");
   check(!inspect.has_class("hotkey-menu-add", "disabled"), "a row with room keeps + usable");
   check(player_file().empty(), "the defaults are not the player's until they change one");
}

void add_and_remove(void *menu)
{
   capture(menu, "menu", "key:f1");
   expect_row("menu", {"Escape", "Home", "L3+R3", "f1"}, "a key captured for MENU");
   expect_status("BINDING SAVED", "a key captured for MENU");
   check(player_file().find("hotkey_menu = \"key:escape pad:home pad:l3+r3 key:f1\"") != std::string::npos,
         "the player's file holds MENU's new list: " + player_file());
   check(inspect.has_class("hotkey-menu-add", "focused"), "focus stays on + after a capture");

   click(menu, "hotkey-menu-4");
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "a chip chosen");
   expect_status("BINDING REMOVED", "a chip chosen");
   check(inspect.has_class("hotkey-menu-3", "focused"),
         "removing the last chip leaves focus on the one before it");

   /* We capture pad inputs by their position on the standard pad, and show
    * a chord in the words for each position. */
   capture(menu, "confirm", "pad:x");
   expect_row("confirm", {"Enter", "Bottom button", "Top button"}, "a pad button captured");
   click(menu, "hotkey-confirm-3");
}

void capture_look_and_ends(void *menu)
{
   click(menu, "hotkey-back-add");
   check(inspect.has_class("hotkey-back-add", "capturing"), "the + being captured is marked capturing");
   check(!rib::hidden(view.document.root()->GetElementById("hotkeys-cancel")),
         "CANCEL shows while a binding is captured");
   /* We draw one thing as focused, the waiting +. CANCEL, which also has
    * the capturing class while the capture runs, looks the same as RESET. */
   check(inspect.has_class("hotkey-back-add", "focused"), "the + being captured has focus");
   check(inspect.property("hotkeys-cancel", "background-color") == inspect.property("hotkeys-reset", "background-color"),
         "CANCEL is drawn as RESET during a capture, not as the focused +: its background is "
         + inspect.property("hotkeys-cancel", "background-color") + ", RESET's "
         + inspect.property("hotkeys-reset", "background-color"));
   expect_status("BACK: PRESS AN INPUT (9)", "a capture counts down in the row's own words");
   check(inspect.words("footer-hint") == "ESC  CANCEL", "the footer says how to cancel: " + inspect.words("footer-hint"));
   {
      Rml::Element *key = view.document.root()->GetElementById("footer-hint")->QuerySelector(".hint-key");
      check(key && rib::words_of(key) == "ESC", "the key the footer names is its own element for the design to style");
   }
   rib_menu_key(menu, RIB_KEY_CANCEL);
   frame(menu);
   expect_status("BINDING UNCHANGED", "the menu's back key during a capture");
   check(!inspect.has_class("hotkey-back-add", "capturing")
         && rib::hidden(view.document.root()->GetElementById("hotkeys-cancel")),
         "a cancelled capture looks done");
   check(inspect.words("footer-hint") == "ESC  BACK", "the footer is the screen's again");

   capture(menu, "back", "key:escape");
   expect_status("BINDING UNCHANGED", "Escape during a capture");
   expect_row("back", {"Escape", "Right button"}, "Escape during a capture");

   capture(menu, "back", "");
   expect_status("USE A KEY OR A PAD BUTTON", "an input that is no key and no pad input");

   click(menu, "hotkey-back-add");
   click(menu, "hotkeys-cancel");
   expect_status("BINDING UNCHANGED", "CANCEL");

   click(menu, "hotkey-back-add");
   host.capture_result = RIB_CAPTURE_TIMED_OUT;
   frame(menu);
   host.capture_result = RIB_CAPTURE_PENDING;
   expect_status("TIMED OUT; BINDING UNCHANGED", "a capture that times out");

   capture(menu, "back", "key:backspace");
   capture(menu, "back", "key:backspace");
   expect_status("BINDING UNCHANGED", "a binding the row already holds");
   click(menu, "hotkey-back-3");
}

void nobody_is_locked_out(void *menu)
{
   click(menu, "hotkey-menu-1");
   expect_status("MENU MUST KEEP A KEY", "removing MENU's only key");
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "a refused removal");

   click(menu, "hotkey-confirm-2");
   click(menu, "hotkey-confirm-1");
   expect_status("CONFIRM MUST KEEP A BINDING", "removing CONFIRM's last binding");
   expect_row("confirm", {"Enter"}, "a refused removal");

   /* Taking the only binding of CONFIRM leaves it with none, and BACK has no
    * key to give back, because Escape also belongs to MENU, so we refuse. */
   capture(menu, "back", "key:enter");
   expect_status("CONFIRM MUST KEEP A BINDING", "a capture that would leave CONFIRM nothing");
   expect_row("confirm", {"Enter"}, "a refused capture");
   expect_row("back", {"Escape", "Right button"}, "a refused capture");

   capture(menu, "confirm", "pad:b");
   expect_row("confirm", {"Enter", "Bottom button"}, "CONFIRM's pad button back");
}

/* Clicking + with the mouse captures the next press. The mouse button that
 * started the capture is released at once, so we accept input for the
 * capture again from the next frame. */
void a_clicked_plus_takes_the_next_press(void *menu)
{
   const int before = host.input_captures_started;
   check(view.document.element_center("hotkey-confirm-add", &host.pointer.x, &host.pointer.y),
         "+ is on screen");
   host.pointer.pressed = true;
   frame(menu);
   host.pointer.pressed = false;
   frame(menu);
   check(host.input_captures_started == before + 1, "a click on + starts a capture");
   frame(menu);
   check(host.capture_accepts_pointer, "the click let go, the capture takes the next press");
   click(menu, "hotkeys-cancel");
   host.pointer = {};
   frame(menu);
}

/* The input that ends a capture is still held when we bind it, so it acts as
 * its new hotkey only after a release and a new press. When it is bound to
 * MENU, its release must not close the screen, and when it is bound to
 * CONFIRM, it must not press the focused element. */
void the_input_that_binds_acts_once_let_go(void *menu)
{
   host.keys_down = {"f9"};
   capture(menu, "confirm", "key:f9");
   expect_row("confirm", {"Enter", "Bottom button", "f9"}, "F9 captured for CONFIRM");
   check(!press({"f9"}, {}).ok, "F9, still held from its capture, does not confirm");
   press({}, {});
   check(press({"f9"}, {}).ok, "let go and pressed again, F9 confirms");
   click(menu, "hotkey-confirm-3");

   host.keys_down = {"f10"};
   capture(menu, "menu", "key:f10");
   const unsigned f10 = key_code("f10");
   const std::vector<unsigned> held = press({"f10"}, {}).menu_keys;
   check(std::find(held.begin(), held.end(), f10) == held.end(),
         "F10, still held from its capture, is not yet one of MENU's keys");
   press({}, {});
   const std::vector<unsigned> later = press({}, {}).menu_keys;
   check(std::find(later.begin(), later.end(), f10) != later.end(), "let go, F10 is one of MENU's keys");
   click(menu, "hotkey-menu-4");

   host.pads_down = {"x"};
   capture(menu, "back", "pad:x");
   check(!press({}, {"x"}).cancel, "a pad button still held from its capture does not go back");
   press({}, {});
   check(press({}, {"x"}).cancel, "let go and pressed again, it goes back");
   click(menu, "hotkey-back-3");
   expect_row("confirm", {"Enter", "Bottom button"}, "after the held inputs");
   expect_row("back", {"Escape", "Right button"}, "after the held inputs");
}

/* The same for a capture on the CONTROLS screen. The bottom button and Enter
 * are CONFIRM in the menu, and the player can bind a control with either.
 * When the player is still pressing that button after the capture ends, we
 * must not press the focused control and start its capture again. */
void the_input_that_binds_a_control_acts_once_let_go(void *menu)
{
   rib_menu_key(menu, RIB_KEY_CANCEL);
   frame(menu);
   click(menu, "controls");
   check(view.screens.current() == "controls", "CONTROLS is open");
   struct Held { std::vector<std::string> keys, pads; const char *name; };
   for (const Held& held : {Held{{}, {"b"}, "the bottom button"}, Held{{"enter"}, {}, "Enter"}})
   {
      const int before = host.captures_started;
      click(menu, "control-up");
      check(host.captures_started == before + 1,
            std::string("clicking the callout of up starts a capture, before binding ") + held.name);
      host.keys_down = held.keys;
      host.pads_down = held.pads;
      host.capture_result = RIB_CAPTURE_CAPTURED;
      frame(menu);
      host.capture_result = RIB_CAPTURE_PENDING;
      act(menu, press(held.keys, held.pads));
      check(host.captures_started == before + 1,
            std::string("we start no second capture while the player is still pressing ")
                  + held.name + " after binding up");
      press({}, {});
      act(menu, press(held.keys, held.pads));
      check(host.captures_started == before + 2,
            std::string("when the player releases ") + held.name
                  + " and presses it again, we start a capture of up");
      rib_menu_key(menu, RIB_KEY_CANCEL);
      frame(menu);
   }
   rib_menu_key(menu, RIB_KEY_CANCEL);
   frame(menu);
   click(menu, "hotkeys");
   check(view.screens.current() == "hotkeys", "HOTKEYS is open again after CONTROLS");
}

void one_capture_swaps_confirm_and_back(void *menu)
{
   capture(menu, "confirm", "pad:a");
   expect_row("confirm", {"Enter", "Right button"}, "the right button captured for CONFIRM");
   expect_row("back", {"Escape", "Bottom button"}, "the right button captured for CONFIRM");
   expect_status("SWAPPED WITH BACK", "the right button captured for CONFIRM");
}

void swapped_buttons_drive_the_menu(void *menu)
{
   const Pressed right = press({}, {"a"});
   check(right.ok && !right.cancel, "after the swap the right button is OK");
   const Pressed bottom = press({}, {"b"});
   check(bottom.cancel && !bottom.ok, "after the swap the bottom button is cancel");
   check(press({"enter"}, {}).ok, "Enter still confirms");

   /* Navigating with them: pressing the bottom button leaves HOTKEYS for
    * Options, and the right button presses the focused element there. */
   act(menu, bottom);
   check(view.screens.current() == "options", "the bottom button goes back to Options, on "
         + view.screens.current());
   view.focus.set("hotkeys");
   act(menu, right);
   check(view.screens.current() == "hotkeys", "the right button opens the focused entry, on "
         + view.screens.current());
}

void what_retroarch_reads(void *menu)
{
   /* MENU's pad bindings: Home alone, L3 and R3 together. */
   check(press({}, {"home"}).menu_pad, "Home opens the menu");
   check(!press({}, {"l3"}).menu_pad && press({}, {"l3", "r3"}).menu_pad, "L3+R3 held together opens the menu");
   check(!press({}, {"a"}).menu_pad, "a CONFIRM button does not open the menu");
   /* Escape is bound to MENU and BACK. It acts once, as MENU, through its keys. */
   const Pressed escape = press({"escape"}, {});
   check(!escape.cancel, "Escape, which MENU also holds, is not BACK's as well");
   check(escape.menu_keys.size() == 1 && escape.menu_keys[0] == key_code("escape"),
         "MENU's keys are Escape alone");
   /* A position bound to a menu hotkey is only that hotkey: L3 no longer
    * reaches the menu as L3, and Start, which no hotkey is bound to, still
    * does. Select, bound to QUICK SAVE, acts only while the game plays, so
    * in the menu it is still Select. */
   capture(menu, "quick-save", "pad:select");
   const uint32_t pressed = (1u << RETRO_DEVICE_ID_JOYPAD_L3) | (1u << RETRO_DEVICE_ID_JOYPAD_START)
         | (1u << RETRO_DEVICE_ID_JOYPAD_SELECT);
   host.pads_down.clear();
   uint32_t buttons = pressed;
   rib_rmlui_menu_buttons(&buttons, RETRO_DEVICE_ID_JOYPAD_A, RETRO_DEVICE_ID_JOYPAD_B);
   check(!(buttons & (1u << RETRO_DEVICE_ID_JOYPAD_L3)) && (buttons & (1u << RETRO_DEVICE_ID_JOYPAD_START)),
         "a position MENU holds is taken out of the menu's buttons; one no hotkey holds stays");
   check(buttons & (1u << RETRO_DEVICE_ID_JOYPAD_SELECT),
         "a position QUICK SAVE holds stays one of the menu's buttons");
   click(menu, "hotkey-quick-save-2");
   expect_row("quick-save", {"f2"}, "QUICK SAVE's pad button removed");

   /* In RetroArch a few keys are buttons of the menu pad, such as Space for
    * Start. A key bound to a menu hotkey is only that hotkey, and RetroArch
    * reads that here. QUICK SAVE acts only while the game plays. */
   check(rib_rmlui_menu_hotkey_key(key_code("enter")) && rib_rmlui_menu_hotkey_key(key_code("escape")),
         "CONFIRM's Enter and MENU's Escape are keys of the menu's hotkeys");
   check(!rib_rmlui_menu_hotkey_key(key_code("space")) && !rib_rmlui_menu_hotkey_key(key_code("f2")),
         "Space, which no hotkey holds, and QUICK SAVE's F2 are not");
   capture(menu, "back", "key:space");
   check(rib_rmlui_menu_hotkey_key(key_code("space")), "Space bound to BACK is a key of the menu's hotkeys");
   click(menu, "hotkey-back-3");
   check(!rib_rmlui_menu_hotkey_key(key_code("space")), "Space taken off BACK is not");
}

/* Seven rows fill two pages of the list in the design: the menu hotkeys and
 * QUICK SAVE on the first, the slot hotkeys on the second, turned with the
 * pager of the list. */
void pages(void *menu)
{
   const auto shown = [](const char *id) {
      Rml::Element *element = view.document.root()->GetElementById(id);
      return element && !rib::hidden(element);
   };
   check(shown("hotkey-menu") && shown("hotkey-quick-save") && !shown("hotkey-quick-load"),
         "the first page shows MENU to QUICK SAVE, and not QUICK LOAD");
   check(shown("hotkeys-pager") && inspect.words("hotkeys-page-count") == "1/2",
         "the pager shows, on 1/2: " + inspect.words("hotkeys-page-count"));
   click(menu, "hotkeys-next");
   check(!shown("hotkey-menu") && shown("hotkey-quick-load") && shown("hotkey-next-slot"),
         "the second page shows QUICK LOAD to NEXT SLOT");
   check(inspect.words("hotkeys-page-count") == "2/2", "the pager says 2/2: " + inspect.words("hotkeys-page-count"));
}

/* QUICK SAVE, QUICK LOAD, PREVIOUS SLOT and NEXT SLOT may be left with no
 * binding. When another hotkey takes a binding from one of them, we move the
 * binding and do not swap, so a hotkey left with nothing gets nothing back. */
void the_hotkeys_of_play_keep_nothing(void *menu)
{
   click(menu, "hotkey-next-slot-1");
   expect_status("BINDING REMOVED", "NEXT SLOT's only binding removed");
   expect_row("next-slot", {}, "NEXT SLOT's only binding removed");
   check(inspect.has_class("hotkey-next-slot-add", "focused"), "focus goes to NEXT SLOT's + when it has none");
   check(player_file().find("hotkey_next-slot = \"\"") != std::string::npos,
         "the player's file holds NEXT SLOT with nothing: " + player_file());

   capture(menu, "previous-slot", "key:f4");
   expect_status("TAKEN FROM QUICK LOAD", "QUICK LOAD's key captured for PREVIOUS SLOT");
   expect_row("previous-slot", {"f6", "f4"}, "QUICK LOAD's key captured for PREVIOUS SLOT");
   expect_row("quick-load", {}, "QUICK LOAD's only key taken");

   click(menu, "hotkey-previous-slot-2");
   capture(menu, "quick-load", "key:f4");
   capture(menu, "next-slot", "key:f7");
   expect_row("quick-load", {"f4"}, "put back");
   expect_row("previous-slot", {"f6"}, "put back");
   expect_row("next-slot", {"f7"}, "put back");
}

void a_full_row(void *menu)
{
   capture(menu, "menu", "key:f1");
   capture(menu, "menu", "pad:select");
   expect_row("menu", {"Escape", "Home", "L3+R3", "f1", "Select"}, "a row filled");
   check(inspect.has_class("hotkey-menu-add", "disabled"), "a full row's + is disabled");
   const int before = host.input_captures_started;
   click(menu, "hotkey-menu-add");
   check(host.input_captures_started == before, "a full row's + starts no capture");
   click(menu, "hotkey-menu-5");
   click(menu, "hotkey-menu-4");

   /* CONFIRM is full and has one pad button. If BACK takes it, CONFIRM has
    * none and would get both of BACK's, but there is no room for them. */
   capture(menu, "confirm", "key:f9");
   capture(menu, "confirm", "key:f10");
   capture(menu, "confirm", "key:f11");
   capture(menu, "back", "pad:y");
   capture(menu, "back", "pad:a");
   expect_status("NO ROOM IN CONFIRM", "a swap that would overfill a row");
   expect_row("confirm", {"Enter", "Right button", "f9", "f10", "f11"}, "a refused swap");
   expect_row("back", {"Escape", "Bottom button", "Left button"}, "a refused swap");
   click(menu, "hotkey-back-3");
   click(menu, "hotkey-confirm-5");
   click(menu, "hotkey-confirm-4");
   click(menu, "hotkey-confirm-3");
}

/* A hotkey that acts while the game plays cannot use any input of the game,
 * or one press would do both: no key bound to a game control, and no pad
 * button bound to one alone. MENU also acts while the game plays. CONFIRM and
 * BACK act only in the menu, where the game is paused, so they may. Every
 * control of the fake game is on host.bound_key, and the first is Up. */
void the_games_inputs_are_not_the_hotkeys_of_play(void *menu)
{
   const std::string game_key = "key:" + host.bound_key;
   capture(menu, "quick-save", game_key.c_str());
   expect_status("THE GAME USES THAT FOR Up", "a key the game reads, for QUICK SAVE");
   expect_row("quick-save", {"f2"}, "a key the game reads, for QUICK SAVE");
   capture(menu, "menu", game_key.c_str());
   expect_status("THE GAME USES THAT FOR Up", "a key the game reads, for MENU");
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "a key the game reads, for MENU");
   /* We read Mega Drive A from the left button. */
   capture(menu, "next-slot", "pad:y");
   expect_status("THE GAME USES THAT FOR A", "a pad button the game reads, for NEXT SLOT");
   expect_row("next-slot", {"f7"}, "a pad button the game reads, for NEXT SLOT");
   capture(menu, "confirm", "pad:y");
   expect_row("confirm", {"Enter", "Right button", "Left button"}, "a pad button the game reads, for CONFIRM");
   click(menu, "hotkey-confirm-3");

   /* A key that a swap would give to MENU: when CONFIRM takes the only key of
    * MENU, MENU would get the keys of CONFIRM, and the game reads one of them. */
   capture(menu, "menu", "key:f1");
   click(menu, "hotkey-menu-1");
   capture(menu, "confirm", game_key.c_str());
   capture(menu, "confirm", "key:f1");
   expect_status("THE GAME USES THAT FOR Up", "a swap that gives MENU a key the game reads");
   expect_row("menu", {"Home", "L3+R3", "f1"}, "a refused swap");
   expect_row("confirm", {"Enter", "Right button", host.bound_key}, "a refused swap");
   click(menu, "hotkey-confirm-3");
}

/* The row of FULLSCREEN starts with the fullscreen chord of the platform,
 * which the player cannot remove, and FULLSCREEN has no other binding until
 * the player adds one. */
void the_fullscreen_chord(void *menu)
{
#if defined(__APPLE__)
   const std::string chord = "OPTION+RETURN";
#else
   const std::string chord = "ALT+ENTER";
#endif
   const auto shown = [] {
      Rml::Element *chip = view.document.root()->GetElementById("hotkey-fullscreen-chord");
      Rml::Element *words = chip ? rib::find_class(chip, "hotkey-words") : nullptr;
      return words ? words->GetInnerRML() : std::string("<no chord>");
   };
   check(shown() == chord, "FULLSCREEN's row starts with " + chord + ", not " + shown());
   expect_row("fullscreen", {}, "the defaults");
   const std::string before = status();
   click(menu, "hotkey-fullscreen-chord");
   check(shown() == chord && status() == before, "clicking the chord changes nothing, and the status says "
         + status());
   capture(menu, "fullscreen", "key:f11");
   expect_row("fullscreen", {"f11"}, "F11 captured for FULLSCREEN");
   check(player_file().find("hotkey_fullscreen = \"key:f11\"") != std::string::npos,
         "the player's file has FULLSCREEN's F11: " + player_file());
   click(menu, "hotkey-fullscreen-1");
   expect_row("fullscreen", {}, "F11 removed from FULLSCREEN");
   check(shown() == chord, "the chord stays after the last binding goes");
}

/* Pressing RESET restores the defaults of the latest export of the game:
 * CONFIRM from the later one, and MENU and BACK, unchanged from the first. */
void reset(void *menu)
{
   click(menu, "hotkeys-reset");
   expect_status("DEFAULTS RESTORED", "RESET");
   expect_row("confirm", {"Space", "Top button"}, "RESET");
   expect_row("back", {"Escape", "Right button"}, "RESET");
   check(!path_is_valid((data + "/hotkeys.cfg").c_str()), "RESET removes the player's file");
}
}

int main(int argc, char **argv)
{
   Utf8Arguments utf8(argc, argv);
   argc = utf8.argc();
   argv = utf8.argv();
   if (argc != 4)
   {
      std::fprintf(stderr, "usage: %s ASSETS OTHER_ASSETS DATA\n", argv[0]);
      return 2;
   }
   data = argv[3];
   test_setenv("ROMINABOX_DATA_DIR", argv[3]);
   test_unsetenv("ROMINABOX_MENU_SCRIPT");

   void *menu = open(argv[1]);
   if (!menu)
      return 1;
   check(view.screens.current() == "hotkeys", "Options opens HOTKEYS");
   defaults_and_words(menu);
   add_and_remove(menu);
   capture_look_and_ends(menu);
   nobody_is_locked_out(menu);
   a_clicked_plus_takes_the_next_press(menu);
   the_input_that_binds_acts_once_let_go(menu);
   the_input_that_binds_a_control_acts_once_let_go(menu);
   one_capture_swaps_confirm_and_back(menu);
   swapped_buttons_drive_the_menu(menu);
   what_retroarch_reads(menu);
   a_full_row(menu);
   pages(menu);
   the_hotkeys_of_play_keep_nothing(menu);
   the_games_inputs_are_not_the_hotkeys_of_play(menu);
   the_fullscreen_chord(menu);
   close(menu);

   /* At the next launch we read the player's file. The swap is still there
    * and still applies in the menu. */
   menu = open(argv[1]);
   expect_row("confirm", {"Enter", "Right button"}, "a relaunch");
   expect_row("back", {"Escape", "Bottom button"}, "a relaunch");
   check(press({}, {"a"}).ok, "after a relaunch the right button is still OK");
   close(menu);

   /* A later export with other defaults does not replace the player's. */
   menu = open(argv[2]);
   expect_row("confirm", {"Enter", "Right button"}, "a later export");
   reset(menu);
   close(menu);
   menu = open(argv[2]);
   expect_row("confirm", {"Space", "Top button"}, "the later export's defaults after RESET");
   close(menu);

   /* We set aside a player's file that would lock the player out. */
   const std::string broken = "hotkey_menu = \"pad:home\"\n";
   filestream_write_file((data + "/hotkeys.cfg").c_str(), broken.data(), (int64_t)broken.size());
   menu = open(argv[1]);
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "a player's file with no key for MENU");
   close(menu);

   if (failures)
      std::fprintf(stderr, "hotkeys: %d failures\n", failures);
   else
      std::printf("hotkeys: every case passed\n");
   return failures ? 1 : 0;
}
