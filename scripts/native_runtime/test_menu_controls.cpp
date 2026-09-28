/* MENU CONTROLS through the production menu, with its document and file
 * layer, on a menu composed by an export: what we show on the screen, adding
 * a binding with the same capture as in Controls, removing one, the rules
 * that nobody can break, the swap, RESET, what survives a relaunch and a
 * later export, and what the menu actions become in RetroArch input. Only
 * the RetroArch host commands are fake.
 *
 *   test_menu_controls ASSETS OTHER_ASSETS DATA
 *
 * ASSETS is a composed Native menu with the builder's defaults. OTHER_ASSETS
 * is the same menu exported again with other defaults (CONFIRM Space and the
 * top button). DATA is an empty folder for this test. */
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
      std::fprintf(stderr, "FAIL menu controls: %s\n", message.c_str());
      ++failures;
   }
}

void frame(void *menu) { rib_menu_frame(menu, 960, 600); }

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
   click(menu, "menu-controls");
   return menu;
}

void close(void *menu)
{
   rib_menu_toggle(menu, false);
   rib_menu_destroy(menu);
}

/* The chips of a row as shown, in order: the words of each visible chip. */
std::vector<std::string> row(const char *action)
{
   std::vector<std::string> shown;
   for (int chip = 1; chip <= 5; ++chip)
   {
      const std::string id = std::string("menu-control-") + action + "-" + std::to_string(chip);
      Rml::Element *element = view.document.root()->GetElementById(id);
      if (element && !rib::hidden(element))
         shown.push_back(inspect.text(id.c_str()));
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

void expect_row(const char *action, const std::vector<std::string>& expected, const char *when)
{
   const std::vector<std::string> shown = row(action);
   check(shown == expected, std::string(when) + ": " + action + " shows [" + joined(shown)
         + "], expected [" + joined(expected) + "]");
}

std::string status() { return inspect.text("menu-controls-status"); }

void expect_status(const char *expected, const char *when)
{
   check(status() == expected, std::string(when) + ": the status says \"" + status()
         + "\", expected \"" + expected + "\"");
}

/* The capture of + on `action`, answered with `input`. */
void capture(void *menu, const char *action, const char *input)
{
   const int before = host.input_captures_started;
   click(menu, (std::string("menu-control-") + action + "-add").c_str());
   check(host.input_captures_started == before + 1,
         std::string("+ on ") + action + " starts a capture");
   host.captured_input = input;
   host.capture_result = RIB_CAPTURE_CAPTURED;
   frame(menu);
   host.capture_result = RIB_CAPTURE_PENDING;
}

std::string player_file()
{
   void *bytes = nullptr;
   int64_t size = 0;
   const std::string path = data + "/menu-controls.cfg";
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
   check(view.document.has_element("menu-controls"), "Options offers MENU CONTROLS");
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "the defaults");
   expect_row("confirm", {"Enter", "Right button"}, "the defaults");
   expect_row("back", {"Escape", "Bottom button"}, "the defaults");
   check(inspect.has_class("menu-control-menu-1", "key") && !inspect.has_class("menu-control-menu-1", "pad"),
         "a key's chip is marked key");
   check(inspect.has_class("menu-control-menu-2", "pad") && !inspect.has_class("menu-control-menu-2", "key"),
         "a pad input's chip is marked pad");
   check(!inspect.has_class("menu-control-menu-add", "disabled"), "a row with room keeps + usable");
   check(player_file().empty(), "the defaults are not the player's until they change one");
}

void add_and_remove(void *menu)
{
   capture(menu, "menu", "key:f1");
   expect_row("menu", {"Escape", "Home", "L3+R3", "f1"}, "a key captured for MENU");
   expect_status("BINDING SAVED", "a key captured for MENU");
   check(player_file().find("menu_control_menu = \"key:escape pad:home pad:l3+r3 key:f1\"") != std::string::npos,
         "the player's file holds MENU's new list: " + player_file());
   check(inspect.has_class("menu-control-menu-add", "focused"), "focus stays on + after a capture");

   click(menu, "menu-control-menu-4");
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "a chip chosen");
   expect_status("BINDING REMOVED", "a chip chosen");
   check(inspect.has_class("menu-control-menu-3", "focused"),
         "removing the last chip leaves focus on the one before it");

   /* We capture pad inputs by their position on the standard pad, and show
    * a chord in the words for each position. */
   capture(menu, "confirm", "pad:x");
   expect_row("confirm", {"Enter", "Right button", "Top button"}, "a pad button captured");
   click(menu, "menu-control-confirm-3");
}

void capture_look_and_ends(void *menu)
{
   click(menu, "menu-control-back-add");
   check(inspect.has_class("menu-control-back-add", "capturing"), "the + being captured is marked capturing");
   check(!rib::hidden(view.document.root()->GetElementById("menu-controls-cancel")),
         "CANCEL shows while a binding is captured");
   expect_status("BACK: PRESS AN INPUT (9)", "a capture counts down in the row's own words");
   check(inspect.text("footer-hint") == "ESC  CANCEL", "the footer says how to cancel: " + inspect.text("footer-hint"));
   rib_menu_key(menu, RIB_KEY_CANCEL);
   frame(menu);
   expect_status("BINDING UNCHANGED", "the menu's back key during a capture");
   check(!inspect.has_class("menu-control-back-add", "capturing")
         && rib::hidden(view.document.root()->GetElementById("menu-controls-cancel")),
         "a cancelled capture looks done");
   check(inspect.text("footer-hint") == "ESC  BACK", "the footer is the screen's again");

   capture(menu, "back", "key:escape");
   expect_status("BINDING UNCHANGED", "Escape during a capture");
   expect_row("back", {"Escape", "Bottom button"}, "Escape during a capture");

   capture(menu, "back", "");
   expect_status("USE A KEY OR A PAD BUTTON", "an input that is no key and no pad input");

   click(menu, "menu-control-back-add");
   click(menu, "menu-controls-cancel");
   expect_status("BINDING UNCHANGED", "CANCEL");

   click(menu, "menu-control-back-add");
   host.capture_result = RIB_CAPTURE_TIMED_OUT;
   frame(menu);
   host.capture_result = RIB_CAPTURE_PENDING;
   expect_status("TIMED OUT; BINDING UNCHANGED", "a capture that times out");

   capture(menu, "back", "key:backspace");
   capture(menu, "back", "key:backspace");
   expect_status("BINDING UNCHANGED", "a binding the row already holds");
   click(menu, "menu-control-back-3");
}

void nobody_is_locked_out(void *menu)
{
   click(menu, "menu-control-menu-1");
   expect_status("MENU NEEDS A KEY", "removing MENU's only key");
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "a refused removal");

   click(menu, "menu-control-confirm-2");
   click(menu, "menu-control-confirm-1");
   expect_status("CONFIRM NEEDS A BINDING", "removing CONFIRM's last binding");
   expect_row("confirm", {"Enter"}, "a refused removal");

   /* Taking the only binding of CONFIRM leaves it with none, and BACK has no
    * key to give back, because Escape also belongs to MENU, so we refuse. */
   capture(menu, "back", "key:enter");
   expect_status("CONFIRM NEEDS A BINDING", "a capture that would leave CONFIRM nothing");
   expect_row("confirm", {"Enter"}, "a refused capture");
   expect_row("back", {"Escape", "Bottom button"}, "a refused capture");

   capture(menu, "confirm", "pad:a");
   expect_row("confirm", {"Enter", "Right button"}, "CONFIRM's pad button back");
}

void one_capture_swaps_confirm_and_back(void *menu)
{
   capture(menu, "confirm", "pad:b");
   expect_row("confirm", {"Enter", "Bottom button"}, "the bottom button captured for CONFIRM");
   expect_row("back", {"Escape", "Right button"}, "the bottom button captured for CONFIRM");
   expect_status("SWAPPED WITH BACK", "the bottom button captured for CONFIRM");
}

void swapped_buttons_drive_the_menu(void *menu)
{
   const Pressed bottom = press({}, {"b"});
   check(bottom.ok && !bottom.cancel, "after the swap the bottom button is OK");
   const Pressed right = press({}, {"a"});
   check(right.cancel && !right.ok, "after the swap the right button is cancel");
   check(press({"enter"}, {}).ok, "Enter still confirms");

   /* Navigating with them: pressing the right button leaves MENU CONTROLS for
    * Options, and the bottom button presses the focused element there. */
   act(menu, right);
   check(view.screens.current() == "options", "the right button goes back to Options, on "
         + view.screens.current());
   view.focus.set("menu-controls");
   act(menu, bottom);
   check(view.screens.current() == "menu-controls", "the bottom button opens the focused entry, on "
         + view.screens.current());
}

void what_retroarch_reads(void *menu)
{
   /* MENU's pad bindings: Home alone, L3 and R3 together. */
   check(press({}, {"home"}).menu_pad, "Home opens the menu");
   check(!press({}, {"l3"}).menu_pad && press({}, {"l3", "r3"}).menu_pad, "L3+R3 held together opens the menu");
   check(!press({}, {"b"}).menu_pad, "a CONFIRM button does not open the menu");
   /* Escape is bound to MENU and BACK. It acts once, as MENU, through its keys. */
   const Pressed escape = press({"escape"}, {});
   check(!escape.cancel, "Escape, which MENU also holds, is not BACK's as well");
   check(escape.menu_keys.size() == 1 && escape.menu_keys[0] == key_code("escape"),
         "MENU's keys are Escape alone");
   /* A position bound to a menu action is only that action: L3 does not
    * reach the menu as L3, and Start, which no action is bound to, does. */
   const uint32_t l3_and_start = (1u << RETRO_DEVICE_ID_JOYPAD_L3) | (1u << RETRO_DEVICE_ID_JOYPAD_START);
   host.pads_down.clear();
   uint32_t buttons = l3_and_start;
   rib_rmlui_menu_buttons(&buttons, RETRO_DEVICE_ID_JOYPAD_A, RETRO_DEVICE_ID_JOYPAD_B);
   check(!(buttons & (1u << RETRO_DEVICE_ID_JOYPAD_L3)) && (buttons & (1u << RETRO_DEVICE_ID_JOYPAD_START)),
         "a position MENU holds is taken out of the menu's buttons; one no action holds stays");
   (void)menu;
}

void a_full_row(void *menu)
{
   capture(menu, "menu", "key:f1");
   capture(menu, "menu", "pad:select");
   expect_row("menu", {"Escape", "Home", "L3+R3", "f1", "Select"}, "a row filled");
   check(inspect.has_class("menu-control-menu-add", "disabled"), "a full row's + is disabled");
   const int before = host.input_captures_started;
   click(menu, "menu-control-menu-add");
   check(host.input_captures_started == before, "a full row's + starts no capture");
   click(menu, "menu-control-menu-5");
   click(menu, "menu-control-menu-4");

   /* CONFIRM is full and has one pad button. If BACK takes it, CONFIRM has
    * none and would get both of BACK's, but there is no room for them. */
   capture(menu, "confirm", "key:f2");
   capture(menu, "confirm", "key:f3");
   capture(menu, "confirm", "key:f4");
   capture(menu, "back", "pad:y");
   capture(menu, "back", "pad:b");
   expect_status("NO ROOM IN CONFIRM", "a swap that would overfill a row");
   expect_row("confirm", {"Enter", "Bottom button", "f2", "f3", "f4"}, "a refused swap");
   expect_row("back", {"Escape", "Right button", "Left button"}, "a refused swap");
   click(menu, "menu-control-back-3");
   click(menu, "menu-control-confirm-5");
   click(menu, "menu-control-confirm-4");
   click(menu, "menu-control-confirm-3");
}

/* Pressing RESET restores the defaults of the latest export of the game:
 * CONFIRM from the later one, and MENU and BACK, unchanged from the first. */
void reset(void *menu)
{
   click(menu, "menu-controls-reset");
   expect_status("DEFAULTS RESTORED", "RESET");
   expect_row("confirm", {"Space", "Top button"}, "RESET");
   expect_row("back", {"Escape", "Bottom button"}, "RESET");
   check(!path_is_valid((data + "/menu-controls.cfg").c_str()), "RESET removes the player's file");
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
   check(view.screens.current() == "menu-controls", "Options opens MENU CONTROLS");
   defaults_and_words(menu);
   add_and_remove(menu);
   capture_look_and_ends(menu);
   nobody_is_locked_out(menu);
   one_capture_swaps_confirm_and_back(menu);
   swapped_buttons_drive_the_menu(menu);
   what_retroarch_reads(menu);
   a_full_row(menu);
   close(menu);

   /* At the next launch we read the player's file. The swap is still there
    * and still applies in the menu. */
   menu = open(argv[1]);
   expect_row("confirm", {"Enter", "Bottom button"}, "a relaunch");
   expect_row("back", {"Escape", "Right button"}, "a relaunch");
   check(press({}, {"b"}).ok, "after a relaunch the bottom button is still OK");
   close(menu);

   /* A later export with other defaults does not replace the player's. */
   menu = open(argv[2]);
   expect_row("confirm", {"Enter", "Bottom button"}, "a later export");
   reset(menu);
   close(menu);
   menu = open(argv[2]);
   expect_row("confirm", {"Space", "Top button"}, "the later export's defaults after RESET");
   close(menu);

   /* We set aside a player's file that would lock the player out. */
   const std::string broken = "menu_control_menu = \"pad:home\"\n";
   filestream_write_file((data + "/menu-controls.cfg").c_str(), broken.data(), (int64_t)broken.size());
   menu = open(argv[1]);
   expect_row("menu", {"Escape", "Home", "L3+R3"}, "a player's file with no key for MENU");
   close(menu);

   if (failures)
      std::fprintf(stderr, "menu controls: %d failures\n", failures);
   else
      std::printf("menu controls: every case passed\n");
   return failures ? 1 : 0;
}
