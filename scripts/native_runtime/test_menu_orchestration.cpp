/* Headless menu orchestration against the document, declarations and file layer
 * of the player. We replace only the RetroArch host commands and runtime state.
 * The tests of the player's settings are in test_menu_player_settings.cpp. */
#include "test_menu_orchestration.hpp"
#include "rmlui/host.h"
#include "rmlui_bridge.h"
#include "rmlui/elements.hpp"
#include "rmlui/overlays.hpp"
#include <file/config_file.h>
#include <file/file_path.h>
#include "test_arguments.h"
#include "test_environment.h"
#include <sys/stat.h>
#include <filesystem>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <sstream>
#include <string>
#include <fstream>
#include <functional>
#include <vector>


namespace {
/* Written through the same file layer. */
bool write_file(const std::string& path, const std::string& text)
{
   return filestream_write_file(path.c_str(), text.data(), (int64_t)text.size());
}

bool binds_visible(int *x, int *y)
{
   return view.document.element_center("control-binds", x, y)
         && view.document.pointer_inside("control-binds", *x, *y);
}

bool status_is(const char *expected)
{
   return inspect.text("status") == expected;
}

/* We draw the ring over a control on the pad lit while the stop for that
 * control has focus, and not while `other` has it. The pointer focuses the
 * stop and then moves to the heading, which is not a stop, so we measure
 * focus and not hover. */
void ring_lights_with_its_stop(void *menu, const char *stop, const char *ring, const char *other)
{
   check(view.document.has_element(ring), (std::string("the scene draws ") + ring).c_str());
   hover_and_frame(menu, other);
   hover_and_frame(menu, "heading");
   check(focused(other), (std::string("the pointer focuses ") + other).c_str());
   const std::string unlit = inspect.property(ring, "border-top-color");
   hover_and_frame(menu, stop);
   hover_and_frame(menu, "heading");
   check(focused(stop), (std::string("the pointer focuses ") + stop).c_str());
   const std::string lit = inspect.property(ring, "border-top-color");
   check(!lit.empty() && lit != unlit,
         (std::string(ring) + " is drawn lit while " + stop + " has focus; it is "
          + lit + " then and " + unlit + " while " + other + " has it").c_str());
}
}

static int capacity_case(const char *assets, const char *data)
{
   /* Without the shared part nothing in the document is a stop, and every
    * check below fails for that one reason. */
   if (!path_is_valid((std::string(assets) + "/parts/navigation.rcss").c_str()))
   {
      std::fprintf(stderr, "FAIL menu capacity: %s has no parts/navigation.rcss; "
            "composition stages the shared parts beside menu.rcss\n", assets);
      return 1;
   }
   test_setenv("ROMINABOX_RML_ASSETS", assets);
   test_setenv("ROMINABOX_DATA_DIR", data);
   test_unsetenv("ROMINABOX_MENU_SCRIPT");
   void *menu = rib_menu_create();
   check(menu != nullptr, "create menu for the large declared profile");
   if (menu)
   {
      frame(menu);
      rib_menu_toggle(menu, true);
      frame(menu);
      std::istringstream expected_file(read_file(std::string(assets) + "/expected-controls.txt"));
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
      /* A stick's ring is over the member that has a place on the pad (L3,
       * R3), which is not the member its box captures. */
      ring_lights_with_its_stop(menu, "control-group-l_stick", "control-hit-l3", "control-group-r_stick");
      ring_lights_with_its_stop(menu, "control-group-r_stick", "control-hit-r3", "control-group-l_stick");
      ring_lights_with_its_stop(menu, "control-l", "control-hit-l", "control-r");
      /* A callout being captured pulses, and so does its ring. */
      hover_and_frame(menu, "control-l");
      rib_menu_key(menu, RIB_KEY_OK);
      check(host.captured_id == "l"
                  && std::string(inspect.property("control-hit-l", "animation")).find("capture-pulse")
                        != std::string::npos,
            "the ring of a callout being captured pulses with it");
      rib_menu_key(menu, RIB_KEY_CANCEL);
      check(std::string(inspect.property("control-hit-l", "animation")).find("capture-pulse")
                  == std::string::npos,
            "the ring stops pulsing when the capture ends");
      rib_menu_destroy(menu);
   }
   if (failures)
      std::fprintf(stderr, "%d menu capacity failures\n", failures);
   return failures ? 1 : 0;
}

/* How we draw a stop and its ring right now: what a design can change to
 * show "this is waiting for input". */
static std::string look(const char *id)
{
   std::string drawn;
   for (const char *name : {"animation", "border-top-color", "border-left-color",
            "border-left-width", "background-color", "color"})
      drawn += std::string(name) + "=" + inspect.property(id, name) + ";";
   return drawn;
}

/* Every stick on an exported pad, in each design that we staged it in. While
 * a stick waits to be rebound, its box and its ring look different from when
 * it is only focused, and they change back when the capture ends. */
static int stick_capture_case(const char *assets, const char *data)
{
   test_setenv("ROMINABOX_RML_ASSETS", assets);
   test_setenv("ROMINABOX_DATA_DIR", data);
   test_unsetenv("ROMINABOX_MENU_SCRIPT");
   void *menu = rib_menu_create();
   check(menu != nullptr, "create a menu for the stick capture case");
   if (!menu)
      return 1;
   frame(menu);
   rib_menu_toggle(menu, true);
   frame(menu);
   click_and_frame(menu, "options");
   click_and_frame(menu, "controls");
   std::vector<std::pair<std::string, std::string>> sticks;
   rib::walk(view.document.root()->GetElementById("controller-scene"), [&](Rml::Element *element) {
      if (!element->IsClassSet("control-group"))
         return rib::Walk::Continue;
      std::string ring;
      rib::walk(element, [&](Rml::Element *child) {
         if (child->IsClassSet("control-hit"))
            ring = child->GetId();
         return rib::Walk::Continue;
      });
      sticks.emplace_back(element->GetId(), ring);
      return rib::Walk::SkipChildren;
   });
   check(sticks.size() == 2, (std::string(assets) + " draws both analogue sticks").c_str());
   for (const auto& [stop, ring] : sticks)
   {
      check(!ring.empty(), (stop + " has a ring on the pad").c_str());
      hover_and_frame(menu, stop.c_str());
      hover_and_frame(menu, "heading");
      check(focused(stop.c_str()), ("the pointer focuses " + stop).c_str());
      const std::string box_focused = look(stop.c_str());
      const std::string ring_focused = look(ring.c_str());
      host.captured_id.clear();
      rib_menu_key(menu, RIB_KEY_OK);
      frame(menu);
      check(!host.captured_id.empty(), ("OK on " + stop + " starts a capture").c_str());
      check(look(stop.c_str()) != box_focused,
            (stop + " shows it is waiting for input; it looks as it does focused: "
             + box_focused).c_str());
      check(look(ring.c_str()) != ring_focused,
            (ring + " shows its stick is waiting for input; it looks as it does focused: "
             + ring_focused).c_str());
      rib_menu_key(menu, RIB_KEY_CANCEL);
      frame(menu);
      check(look(stop.c_str()) == box_focused && look(ring.c_str()) == ring_focused,
            (stop + " and its ring look focused again once the capture ends").c_str());
   }
   rib_menu_destroy(menu);
   if (failures)
      std::fprintf(stderr, "%d stick capture failures in %s\n", failures, assets);
   return failures ? 1 : 0;
}

/* Each of these cases opens its own menu. */
namespace fixes {
/* When we first paint an empty status, the prompt from the Disc design stays. */
void design_prompt_survives_an_empty_status(const char *native_assets, const char *data)
{
   const std::string assets = design_assets(native_assets, "disc");
   check(std::filesystem::is_regular_file(assets + "/menu.rml"), "the Disc design is staged");
   test_setenv("ROMINABOX_RML_ASSETS", assets.c_str());
   void *menu = open_menu();
   if (menu)
   {
      /* The earlier cases left a status in this process. Let it expire. */
      inspect.advance(6.0);
      frame(menu);
      click_and_frame(menu, "slot-2");
      check(std::string(inspect.text("status")) == "CHOOSE A BLOCK",
            "Disc's prompt is shown while there is no status");
      host.save_accepted = false;
      click_and_frame(menu, "save");
      check(std::string(inspect.text("status")) == "SAVE FAILED", "a status replaces the prompt");
      inspect.advance(6.0);
      frame(menu);
      check(std::string(inspect.text("status")) == "CHOOSE A BLOCK",
            "Disc's prompt comes back when the status expires");
      rib_menu_destroy(menu);
   }
   test_setenv("ROMINABOX_RML_ASSETS", native_assets);
   (void)data;
}

/* Choosing a row on a list screen runs the action of that list. A row of the
 * filter list applies its filter, and we do not call the list of another
 * screen. We stage a whole Native menu, with filters, next to the others. */
void a_filter_row_applies_its_filter(const char *native_assets, const char *data)
{
   const std::string assets = design_assets(native_assets, "everything");
   check(std::filesystem::is_regular_file(assets + "/shaders.cfg"), "the menu with filters is staged");
   test_setenv("ROMINABOX_RML_ASSETS", assets.c_str());
   host.applied_shader.clear();
   host.applied_preset.clear();
   host.applied_while_drawing = 0;
   void *menu = open_menu();
   if (menu)
   {
      click_and_frame(menu, "options");
      click_and_frame(menu, "shaders");
      check(std::string(inspect.text("heading")) == "SHADERS", "SHADERS opens the filter list");
      click_and_frame(menu, "scanlines");
      check(host.applied_shader == "scanlines", "choosing a filter row applies that filter");
      check(host.applied_while_drawing == 0,
            "a filter is applied between frames, never while the video driver draws the menu");
      const std::string preset = "/shaders/scanlines/scanlines.glslp";
      check(host.applied_preset.size() > preset.size() &&
            host.applied_preset.compare(host.applied_preset.size() - preset.size(), preset.size(), preset) == 0,
            "the filter is applied with the preset the export named");
      /* We store it by id, because a path would contain the folder this copy
       * of the game was unpacked into, and the next export replaces that. */
      check(read_file(std::string(data) + "/shader-choice") == "scanlines\n",
            "the chosen filter is kept by its id, not by where its file is");
      rib_menu_destroy(menu);
   }
   test_setenv("ROMINABOX_RML_ASSETS", native_assets);
}


/* The fixture folder `name` in `data`: the files in `from`, and the shared
 * parts that every menu.rml links, from `native`, next to menu.rcss, as we
 * stage them in composition. */
std::filesystem::path stage_fixture(const std::filesystem::path& from,
      const std::filesystem::path& native, const char *data, const char *name)
{
   namespace fs = std::filesystem;
   const fs::path assets = fs::path(data) / name;
   fs::create_directories(assets);
   for (const auto& entry : fs::directory_iterator(from))
      if (entry.is_regular_file())
         fs::copy_file(entry.path(), assets / entry.path().filename(), fs::copy_options::overwrite_existing);
   fs::copy(native / "parts", assets / "parts", fs::copy_options::recursive | fs::copy_options::overwrite_existing);
   return assets;
}

bool replace_once(std::string& text, const std::string& from, const std::string& to)
{
   const auto at = text.find(from);
   if (at == std::string::npos) return false;
   text.replace(at, from.size(), to);
   return true;
}

/* The Native assets with the disc list we write in an export for a disc game:
 * eight rows, five to a page, and a hidden DISC entry in Options. */
std::string stage_disc_list(const char *native_assets, const char *data)
{
   namespace fs = std::filesystem;
   const fs::path assets = stage_fixture(native_assets, native_assets, data, "disc-list-assets");
   std::string rows;
   for (int index = 0; index < 8; ++index)
   {
      const std::string id = "discs-" + std::to_string(index);
      rows += "<button id=\"" + id + "\" class=\"list-row line \"><div id=\"" + id
            + "-title\" class=\"list-row-title\"></div><div id=\"" + id
            + "-state\" class=\"list-row-state\"></div></button>\n";
   }
   const std::string panel =
         "<div id=\"discs-panel\" class=\"screen-panel\" style=\"display:none;\"><div id=\"discs-list\" class=\"list\" data-page-size=\"5\">"
         "<div class=\"list-page\">" + rows + "</div>"
         "<div id=\"discs-pager\" class=\"list-pager\" style=\"display:none;\"><button id=\"discs-prev\" class=\"menu-action list-pager-prev\">&lt;</button>"
         "<div id=\"discs-page-count\" class=\"list-pager-count\"></div>"
         "<button id=\"discs-next\" class=\"menu-action list-pager-next\">&gt;</button></div></div>"
         "<div class=\"list-actions\"><button class=\"menu-action list-back\" id=\"discs-back\">BACK</button></div>"
         "<div id=\"discs-status\" class=\"list-status\"></div></div>";
   std::string menu = read_file(assets / "menu.rml");
   bool staged = replace_once(menu, "<div id=\"footer\">", panel + "<div id=\"footer\">")
         && replace_once(menu, "<div id=\"options-entries\">",
               "<div id=\"options-entries\"><button class=\"menu-action option-entry\" id=\"discs\" disabled=\"disabled\" style=\"display: none;\"><span class=\"option-label\">DISC</span></button>");
   std::string config = read_file(assets / "design.cfg");
   /* We add the disc list to the screens that the design declares, which end
    * with the screen for the platform (UNINSTALL or RESET). */
   const size_t screens = config.find("screens = \"");
   const size_t screens_end = screens == std::string::npos ? screens : config.find('"', screens + 11);
   staged = staged && screens_end != std::string::npos;
   if (staged)
      config.insert(screens_end, " discs");
   staged = staged && replace_once(config, "screen_button_options = \"options fixture-back\"",
               "screen_button_options = \"options fixture-back discs-back\"");
   config += "\nscreen_panel_discs = \"discs-panel\"\nscreen_heading_discs = \"DISC\""
             "\nscreen_footer_discs = \"ESC  BACK\"\nscreen_button_discs = \"discs\""
             "\nscreen_role_discs = \"discs\"\n";
   check(staged, "the disc list fixture is staged into the Native assets");
   std::ofstream(assets / "menu.rml") << menu;
   std::ofstream(assets / "design.cfg") << config;
   return assets.string();
}

/* The disc list stays on the chosen page across frames, so disc 6 and
 * later can be chosen. */
void disc_list_keeps_its_page(const char *native_assets, const char *data)
{
   const std::string assets = stage_disc_list(native_assets, data);
   test_setenv("ROMINABOX_RML_ASSETS", assets.c_str());
   host.disc_count = 7;
   host.disc_index = 0;
   void *menu = open_menu();
   if (menu)
   {
      click_and_frame(menu, "options");
      click_and_frame(menu, "discs");
      check(std::string(inspect.text("heading")) == "DISC", "seven discs open the disc list");
      check(std::string(inspect.text("discs-page-count")) == "1/2",
            "the rows the export wrote on one page are split five to a page");
      click_and_frame(menu, "discs-next");
      check(std::string(inspect.text("discs-page-count")) == "2/2", "the pager turns to the second page");
      frame(menu);
      frame(menu);
      int x, y, w, h;
      check(std::string(inspect.text("discs-page-count")) == "2/2" && inspect.box("discs-5", &x, &y, &w, &h),
            "the second page stays open across frames");
      click_and_frame(menu, "discs-5");
      check(host.disc_index == 5, "disc 6 can be chosen");
      rib_menu_destroy(menu);
   }
   host.disc_count = 0;
   host.disc_index = 0;
   test_setenv("ROMINABOX_RML_ASSETS", native_assets);
}

/* When the pointer rests on a control, we open its bindings list after
 * 300 ms, and after a keyboard move we wait the longer keyboard delay. */
void binds_open_sooner_on_hover()
{
   void *menu = open_menu();
   if (!menu) return;
   int x = 0, y = 0;
   click_and_frame(menu, "options");
   click_and_frame(menu, "controls");
   host.clock_us = 10000000;
   /* Not the control the screen opened on, whose timer started long ago. */
   hover_and_frame(menu, "control-down");
   host.clock_us += 250000;
   frame(menu);
   check(!binds_visible(&x, &y), "the bindings list is still closed 250 ms after a hover");
   host.clock_us += 100000;
   frame(menu);
   check(binds_visible(&x, &y), "the bindings list opens 350 ms after a hover");

   host.pointer.x = 0;
   host.pointer.y = 0;
   frame(menu);
   rib_menu_key(menu, RIB_KEY_DOWN);
   frame(menu);
   host.clock_us += 350000;
   frame(menu);
   check(!binds_visible(&x, &y), "a keyboard move waits longer than a hover");
   host.clock_us += 900000;
   frame(menu);
   check(binds_visible(&x, &y), "a keyboard move opens the list after the declared delay");
   rib_menu_destroy(menu);
   host.clock_us = 0;
}

/* An exported Mega Drive game with the 3- and 6-button pads: the staged
 * document and scenes, with the defaults that we write in an export. */
std::string stage_pad_choice(const char *native_assets, const char *data)
{
   namespace fs = std::filesystem;
   const fs::path native(native_assets);
   const fs::path assets = stage_fixture(native / "stage" / "megadrive", native, data, "pad-choice-assets");
   for (const char *support : {"menu.rcss", "Silkscreen-Regular.ttf"})
      fs::copy_file(native / support, assets / support, fs::copy_options::overwrite_existing);
   std::ofstream(assets / "controls-defaults.cfg") <<
      "controls_profile = \"megadrive\"\n"
      "controls_variants = \"megadrive megadrive6\"\n"
      "controls_variant_name_megadrive = \"Mega Drive 3 buttons\"\n"
      "controls_variant_device_megadrive = \"257\"\n"
      "controls_variant_controls_megadrive = \"up left right down y b a start\"\n"
      "controls_variant_name_megadrive6 = \"Mega Drive 6 buttons\"\n"
      "controls_variant_device_megadrive6 = \"513\"\n"
      "controls_variant_controls_megadrive6 = \"up down left right y b a start l x r select\"\n"
      "rib_label_up = \"Up\"\ninput_player1_up = \"up\"\n"
      "rib_label_left = \"Left\"\ninput_player1_left = \"left\"\n"
      "rib_label_right = \"Right\"\ninput_player1_right = \"right\"\n"
      "rib_label_down = \"Down\"\ninput_player1_down = \"down\"\n"
      "rib_label_y = \"A\"\ninput_player1_y = \"z\"\n"
      "rib_label_b = \"B\"\ninput_player1_b = \"x\"\n"
      "rib_label_a = \"C\"\ninput_player1_a = \"c\"\n"
      "rib_label_start = \"Start\"\ninput_player1_start = \"enter\"\n"
      "rib_label_l = \"X\"\ninput_player1_l = \"a\"\n"
      "rib_label_x = \"Y\"\ninput_player1_x = \"s\"\n"
      "rib_label_r = \"Z\"\ninput_player1_r = \"d\"\n"
      "rib_label_select = \"Mode\"\ninput_player1_select = \"rshift\"\n";
   return assets.string();
}

bool picker_open()
{
   int x, y, w, h;
   return inspect.box("controls-device-list", &x, &y, &w, &h);
}

/* One click on a control starts exactly one capture of that control. */
void click_captures_once(void *menu, const char *control, const char *id, const char *message)
{
   const int before = host.captures_started;
   click_and_frame(menu, control);
   check(host.captures_started == before + 1 && host.captured_id == id, message);
   rib_menu_key(menu, RIB_KEY_CANCEL);
   frame(menu);
}

/* After a pad change the picker has one toggle listener and opens. Reset
 * restores the 3-button labels together with the 3-button picture. */
void pad_changes_and_reset_apply_together(const char *native_assets, const char *data)
{
   const std::string assets = stage_pad_choice(native_assets, data);
   const std::string data_dir = std::string(data) + "/pad-choice-data";
   std::filesystem::create_directories(data_dir);
   test_setenv("ROMINABOX_RML_ASSETS", assets.c_str());
   test_setenv("ROMINABOX_DATA_DIR", data_dir.c_str());
   host.pointer = {};
   void *menu = open_menu();
   if (menu)
   {
      click_and_frame(menu, "options");
      click_and_frame(menu, "controls");
      click_captures_once(menu, "control-up", "up", "a control click starts one capture after startup");
      click_and_frame(menu, "controls-device-current");
      check(picker_open(), "B5: the picker opens");
      click_and_frame(menu, "controls-device-option-megadrive6");
      check(!picker_open() && view.document.has_element("control-x"),
            "choosing the 6-button pad draws it");
      check(std::string(inspect.text("controls-device-current")) == "Mega Drive 6 buttons",
            "the picker names the 6-button pad");
      check(host.applied_device == "megadrive6" && host.applied_libretro == 513,
            "choosing the 6-button pad hands its device to the core");
      click_and_frame(menu, "controls-device-current");
      check(picker_open(), "the picker opens again after a pad change");
      click_and_frame(menu, "controls-device-current");
      check(!picker_open(), "the picker closes again");
      click_captures_once(menu, "control-x", "x", "a 6-button control click starts one capture");
      click_captures_once(menu, "control-up", "up", "a control click starts one capture after a pad change");

      click_and_frame(menu, "controls-reset");
      check(!view.document.has_element("control-x") && view.document.has_element("control-y"),
            "Reset draws the 3-button pad again");
      check(std::string(inspect.text("controls-device-current")) == "Mega Drive 3 buttons",
            "Reset names the 3-button pad");
      check(std::string(inspect.text("control-label-y")) == "A", "Reset restores the 3-button labels");
      check(host.applied_device == "megadrive" && host.applied_libretro == 257,
            "Reset hands the 3-button pad's device back to the core");
      click_and_frame(menu, "controls-device-current");
      check(picker_open(), "the picker opens after Reset");
      click_and_frame(menu, "controls-device-current");
      click_captures_once(menu, "control-up", "up", "a control click starts one capture after Reset");
      click_and_frame(menu, "controls-device-current");
      click_and_frame(menu, "controls-device-option-megadrive6");
      rib_menu_destroy(menu);
   }
   /* At the next launch we show the pad the player chose, not the exported one. */
   if ((menu = open_menu()))
   {
      check(view.document.has_element("control-x")
            && std::string(inspect.text("controls-device-current")) == "Mega Drive 6 buttons",
            "the next launch draws and names the pad the player chose");
      rib_menu_destroy(menu);
   }
   test_setenv("ROMINABOX_RML_ASSETS", native_assets);
   test_setenv("ROMINABOX_DATA_DIR", data);
}

/* While nobody touches an open menu, we build no new geometry. RmlUi builds
 * geometry only for what a change makes it lay out or draw again. We add a
 * save state without telling the menu, which never happens in the player
 * (we announce a save when it finishes, and look again when the menu opens),
 * to show whether we keep requesting the slot files. */
void an_idle_menu_builds_nothing(const char *native_assets)
{
   test_setenv("ROMINABOX_RML_ASSETS", native_assets);
   host.slot_occupied = false;
   host.pointer = {};
   host.clock_us = 1000000;
   void *menu = open_menu();
   if (!menu) return;
   const auto idle = [&](const char *where, const std::function<void()>& meanwhile) {
      for (int settle = 0; settle < 3; ++settle)
         frame(menu);
      const unsigned before = view.document.geometry_compiled();
      meanwhile();
      for (int count = 0; count < 20; ++count)
         frame(menu);
      const unsigned built = view.document.geometry_compiled() - before;
      char message[256];
      std::snprintf(message, sizeof(message),
            "%s: 20 frames nobody touched built %u pieces of geometry", where, built);
      check(built == 0, message);
   };
   idle("pause", [] {});
   idle("pause, while a state file appears unannounced", [] { host.slot_occupied = true; });
   check(!inspect.has_class("slot-1", "occupied"),
         "a menu nobody touches does not keep looking at the slots' files");
   rib_menu_toggle(menu, false);
   rib_menu_toggle(menu, true);
   frame(menu);
   check(inspect.has_class("slot-1", "occupied"), "opening the menu looks at the slots");
   click_and_frame(menu, "options");
   idle("options, with the volume slider", [] {});
   click_and_frame(menu, "controls");
   idle("controls", [] {});
   click_and_frame(menu, "controls-back");
   click_and_frame(menu, "fixture");
   idle("a list screen", [] {});
   rib_menu_destroy(menu);
   host.slot_occupied = false;
}

/* A save over a slot that already has a picture. RetroArch reports the save
 * (save_state_cb) before it writes the new screenshot, so at the report the
 * slot still has the picture of the previous save, and the new one arrives a
 * few frames later. We show it in the open menu once it is there. This is a
 * separate game in which no slot was chosen, so it saves to slot 1. */
void a_save_over_a_picture_shows_the_new_one(const char *data)
{
   const std::string picture = std::string(data) + "/resave-slot-1.png";
   const std::string data_dir = std::string(data) + "/resave-data";
   std::filesystem::create_directories(data_dir);
   test_setenv("ROMINABOX_DATA_DIR", data_dir.c_str());
   write_file(picture, "the first save's picture");
   host.slot_occupied = true;
   host.thumbnail = picture;
   host.save_accepted = true;
   void *menu = open_menu();
   if (!menu) return;
   for (int settle = 0; settle < 3; ++settle)
      frame(menu);
   check(view.slots.has_thumbnail(1), "the occupied slot shows its picture");
   const unsigned before = inspect.texture_loads();
   click_and_frame(menu, "save");
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, true, true);
   for (int waiting = 0; waiting < 3; ++waiting)
      frame(menu);
   write_file(picture, "the second save's picture, written after the report");
   for (int after = 0; after < 3; ++after)
      frame(menu);
   check(inspect.texture_loads() > before,
         "a save over a slot with a picture shows the new picture once RetroArch has "
         "written it, without the menu being reopened");
   rib_menu_destroy(menu);
   std::remove(picture.c_str());
   host.slot_occupied = false;
   host.thumbnail.clear();
   test_setenv("ROMINABOX_DATA_DIR", data);
}

/* While the menu is open we run no frame of the game. Right after a load
 * from the menu, we have no frame of the loaded state to take a picture of,
 * and on the Mega Drive the picture is black. So for a save at that moment
 * we copy the picture of the slot we loaded. Once the menu has closed, we
 * run frames of the game again, and for a save we take a picture of the
 * last frame. */
void a_save_right_after_a_load_gets_the_loaded_picture(const char *data)
{
   const std::string picture = std::string(data) + "/loaded-slot-1.png";
   const std::string data_dir = std::string(data) + "/loaded-picture-data";
   std::filesystem::create_directories(data_dir);
   test_setenv("ROMINABOX_DATA_DIR", data_dir.c_str());
   write_file(picture, "the picture of slot 1");
   host.slot_occupied = true;
   host.thumbnail = picture;
   host.load_accepted = true;
   host.save_accepted = true;
   host.picture_copies.clear();
   void *menu = open_menu();
   if (!menu) return;
   frame(menu);
   click_and_frame(menu, "load");
   rib_rmlui_notify_state_task(host.state_path.c_str(), 1, false, true);
   frame(menu);
   click_and_frame(menu, "slot-2");
   click_and_frame(menu, "save");
   rib_rmlui_notify_state_task("", 2, true, true);
   frame(menu);
   check(host.picture_copies == std::vector<std::pair<int, int>>{{1, 2}},
         "we copy the picture of slot 1 for a save to slot 2 right after loading slot 1");

   rib_menu_toggle(menu, false);
   frame(menu);
   rib_menu_toggle(menu, true);
   frame(menu);
   const size_t copied = host.picture_copies.size();
   click_and_frame(menu, "slot-3");
   click_and_frame(menu, "save");
   rib_rmlui_notify_state_task("", 3, true, true);
   frame(menu);
   check(host.picture_copies.size() == copied,
         "after the menu has closed and opened again, we copy no picture for a save");
   rib_menu_destroy(menu);
   std::remove(picture.c_str());
   host.slot_occupied = false;
   host.thumbnail.clear();
   host.load_accepted = false;
   host.save_accepted = false;
   test_setenv("ROMINABOX_DATA_DIR", data);
}
}

int main(int argc, char **argv)
{
   /* Paths arrive as UTF-8 on every platform. */
   Utf8Arguments utf8(argc, argv);
   argc = utf8.argc();
   argv = utf8.argv();
   if (argc == 4 && std::strcmp(argv[1], "--capacity") == 0)
      return capacity_case(argv[2], argv[3]);
   if (argc == 4 && std::strcmp(argv[1], "--stick-capture") == 0)
      return stick_capture_case(argv[2], argv[3]);
   if (argc != 3 || !argv[1][0] || !argv[2][0])
   {
      std::fprintf(stderr, "usage: %s staged-assets owned-data-dir\n"
            "       %s --capacity staged-large-profile owned-data-dir\n"
            "       %s --stick-capture staged-pad-with-sticks owned-data-dir\n",
            argv[0], argv[0], argv[0]);
      return 2;
   }
   test_setenv("ROMINABOX_RML_ASSETS", argv[1]);
   test_setenv("ROMINABOX_DATA_DIR", argv[2]);
   test_unsetenv("ROMINABOX_MENU_SCRIPT");

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
   int sliders = 0;
   for (Rml::Element *stop : view.focus.stops(view.document.root()->GetElementById("options-panel"))) {
      check(!stop->IsClassSet("volume-arrow"), "volume arrows are pointer-only targets");
      if (view.parts.part_is_slider(stop->GetId().c_str())) ++sliders;
   }
   check(sliders == 1, "volume has one logical keyboard/joypad stop");
   /* The slider is the first stop in the Options document, so we focus it
    * when the screen opens. Left and Right then move the level, not the focus. */
   check(focused("volume-level"), "Options opens on the volume slider");
   {
      const float before = host.settings["audio_volume"];
      rib_menu_key(menu, RIB_KEY_RIGHT);
      frame(menu);
      check(host.settings["audio_volume"] > before && focused("volume-level"), "Right changes volume without leaving its control");
      rib_menu_key(menu, RIB_KEY_LEFT);
      frame(menu);
      check(std::fabs(host.settings["audio_volume"] - before) < 0.06f && focused("volume-level"), "Left restores volume without an arrow focus stop");
   }
   check(view.parts.commit_slider("volume-level", 0.5f),
         "the staged Options screen exposes its volume slider");
   frame(menu);
   const std::string volume_path = std::string(argv[2]) + "/volume.cfg";
   config_file_t *volume_file = config_file_new_from_path_to_string(volume_path.c_str());
   float saved_volume = 0.0f;
   check(volume_file && config_get_float(volume_file, "audio_volume", &saved_volume)
         && std::fabs(saved_volume - host.settings["audio_volume"]) < 0.06f,
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
      /* When the player goes fullscreen and back while the game plays, the
       * menu gets a new video driver, and we draw nothing until it opens.
       * Escape must not open the menu before we build its document again, or
       * the player crashes in ElementDocument::GetContext. */
      rib_menu_toggle(menu, false);
      rib_menu_context_destroy(menu);
      rib_menu_context_reset(menu);
      rib_menu_toggle(menu, true);
      frame(menu);
      check(view.document.has_element("save"), "a menu opened before its document is built again opens");
      /* A key in that gap must not reach the menu's focus through the old
       * document, which ElementDocument::GetContext and
       * Context::GetFocusElement would then read. */
      rib_menu_toggle(menu, false);
      rib_menu_context_destroy(menu);
      rib_menu_context_reset(menu);
      rib_menu_toggle(menu, true);
      rib_menu_key(menu, RIB_KEY_CANCEL);
      rib_menu_key(menu, RIB_KEY_DOWN);
      frame(menu);
      check(view.document.has_element("save"), "a key before the document is built again is ignored");
      /* Going fullscreen while the game runs with something drawn over it. We
       * build the new document in a frame of the running game, and the pause
       * screen must not appear over it. */
      rib_menu_toggle(menu, false);
      host.menu_open = false;
      frame(menu);
      check(view.document.root()->IsClassSet("overlay"), "a closed menu draws only its overlays");
      rib_menu_context_destroy(menu);
      rib_menu_context_reset(menu);
      frame(menu);
      check(view.document.root()->IsClassSet("overlay"),
            "a document built again while the game runs draws only its overlays");
      /* Something was drawn over the game (its splash, a notice) and then the
       * menu opened. The screen entered while the document showed only
       * overlays, where nothing can take focus, and focus must still reach an
       * element of the screen. */
      host.menu_open = true;
      rib_menu_toggle(menu, true);
      frame(menu);
      check(focused("resume"), "a menu opened after the game drew over itself starts on CONTINUE");
      rib_menu_destroy(menu);
   }

   fixes::an_idle_menu_builds_nothing(argv[1]);
   fixes::repeated_saves_replace_the_file(argv[2]);
   fixes::background_play_is_the_players(argv[1], argv[2]);
   fixes::rumble_is_the_players_where_the_game_rumbles(argv[1], argv[2]);
   fixes::volume_is_heard_at_its_level(argv[1]);
   fixes::design_prompt_survives_an_empty_status(argv[1], argv[2]);
   fixes::disc_list_keeps_its_page(argv[1], argv[2]);
   fixes::a_filter_row_applies_its_filter(argv[1], argv[2]);
   fixes::binds_open_sooner_on_hover();
   fixes::pad_changes_and_reset_apply_together(argv[1], argv[2]);
   fixes::chosen_slot_shows_on_save_and_load(argv[1], argv[2]);
   fixes::a_save_over_a_picture_shows_the_new_one(argv[2]);
   fixes::a_save_right_after_a_load_gets_the_loaded_picture(argv[2]);
   fixes::a_menu_load_writes_the_volume_only_off_a_position();
   fixes::a_drag_cut_short_by_closing_is_kept(argv[2]);
   fixes::the_middle_of_the_volume_is_clearly_audible();

   if (failures)
      std::fprintf(stderr, "%d menu orchestration failures\n", failures);
   return failures ? 1 : 0;
}
