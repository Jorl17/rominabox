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
#include "rmlui/files.h"
#include <sys/stat.h>
#include <filesystem>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <fstream>
#include <functional>
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
   if (!std::ifstream(std::string(assets) + "/parts/navigation.rcss"))
   {
      std::fprintf(stderr, "FAIL menu capacity: %s has no parts/navigation.rcss; "
            "composition stages the shared parts beside menu.rcss\n", assets);
      return 1;
   }
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
   setenv("ROMINABOX_RML_ASSETS", assets, 1);
   setenv("ROMINABOX_DATA_DIR", data, 1);
   unsetenv("ROMINABOX_MENU_SCRIPT");
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
void *open_menu()
{
   void *menu = rib_menu_create();
   check(menu != nullptr, "create a menu for a fix case");
   if (!menu) return nullptr;
   frame(menu);
   rib_menu_toggle(menu, true);
   frame(menu);
   return menu;
}

/* We stage every design next to the Native assets in the bridge script. */
std::string design_assets(const char *native_assets, const char *design)
{
   return (std::filesystem::path(native_assets).parent_path() / (std::string("placement-") + design)).string();
}

/* When we first paint an empty status, the prompt from the Disc design stays. */
void design_prompt_survives_an_empty_status(const char *native_assets, const char *data)
{
   const std::string assets = design_assets(native_assets, "disc");
   check(std::filesystem::is_regular_file(assets + "/menu.rml"), "the Disc design is staged");
   setenv("ROMINABOX_RML_ASSETS", assets.c_str(), 1);
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
   setenv("ROMINABOX_RML_ASSETS", native_assets, 1);
   (void)data;
}

/* Choosing a row on a list screen runs the action of that list. A row of the
 * filter list applies its filter, and we do not call the list of another
 * screen. We stage a whole Native menu, with filters, next to the others. */
void a_filter_row_applies_its_filter(const char *native_assets)
{
   const std::string assets = design_assets(native_assets, "everything");
   check(std::filesystem::is_regular_file(assets + "/shaders.cfg"), "the menu with filters is staged");
   setenv("ROMINABOX_RML_ASSETS", assets.c_str(), 1);
   host.applied_shader.clear();
   host.applied_preset.clear();
   void *menu = open_menu();
   if (menu)
   {
      click_and_frame(menu, "options");
      click_and_frame(menu, "shaders");
      check(std::string(inspect.text("heading")) == "SHADERS", "SHADERS opens the filter list");
      click_and_frame(menu, "scanlines");
      check(host.applied_shader == "scanlines", "choosing a filter row applies that filter");
      const std::string preset = "/shaders/scanlines/scanlines.glslp";
      check(host.applied_preset.size() > preset.size() &&
            host.applied_preset.compare(host.applied_preset.size() - preset.size(), preset.size(), preset) == 0,
            "the filter is applied with the preset the export named");
      rib_menu_destroy(menu);
   }
   setenv("ROMINABOX_RML_ASSETS", native_assets, 1);
}

std::string read_file(const std::filesystem::path& path)
{
   std::ifstream in(path);
   return std::string(std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>());
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
   const fs::path assets = fs::path(data) / "disc-list-assets";
   fs::create_directories(assets);
   for (const auto& entry : fs::directory_iterator(native_assets))
      if (entry.is_regular_file())
         fs::copy_file(entry.path(), assets / entry.path().filename(), fs::copy_options::overwrite_existing);
   std::string rows[2];
   for (int index = 0; index < 8; ++index)
   {
      const std::string id = "discs-" + std::to_string(index);
      rows[index < 5 ? 0 : 1] += "<button id=\"" + id + "\" class=\"list-row line \"><div id=\"" + id
            + "-title\" class=\"list-row-title\"></div><div id=\"" + id
            + "-state\" class=\"list-row-state\"></div></button>\n";
   }
   const std::string panel =
         "<div id=\"discs-panel\" class=\"screen-panel\" style=\"display:none;\"><div id=\"discs-list\" class=\"list\">"
         "<div id=\"discs-page-1\" class=\"list-page\">" + rows[0] + "</div>"
         "<div id=\"discs-page-2\" class=\"list-page\" style=\"display:none;\">" + rows[1] + "</div>"
         "<div id=\"discs-pager\" class=\"list-pager\"><button id=\"discs-prev\" class=\"menu-action list-pager-prev disabled\">&lt;</button>"
         "<div id=\"discs-page-count\" class=\"list-pager-count\">1/2</div>"
         "<button id=\"discs-next\" class=\"menu-action list-pager-next\">&gt;</button></div></div>"
         "<div class=\"list-actions\"><button class=\"menu-action list-back\" id=\"discs-back\">BACK</button></div>"
         "<div id=\"discs-status\" class=\"list-status\"></div></div>";
   std::string menu = read_file(assets / "menu.rml");
   bool staged = replace_once(menu, "<div id=\"footer\">", panel + "<div id=\"footer\">")
         && replace_once(menu, "<div id=\"options-entries\">",
               "<div id=\"options-entries\"><button class=\"menu-action option-entry\" id=\"discs\" disabled=\"disabled\" style=\"display: none;\"><span class=\"option-label\">DISC</span></button>");
   std::string config = read_file(assets / "design.cfg");
   staged = staged && replace_once(config, "screens = \"pause options controls fixture\"",
               "screens = \"pause options controls fixture discs\"")
         && replace_once(config, "screen_button_options = \"options fixture-back\"",
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
   setenv("ROMINABOX_RML_ASSETS", assets.c_str(), 1);
   host.disc_count = 7;
   host.disc_index = 0;
   void *menu = open_menu();
   if (menu)
   {
      click_and_frame(menu, "options");
      click_and_frame(menu, "discs");
      check(std::string(inspect.text("heading")) == "DISC", "seven discs open the disc list");
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
   setenv("ROMINABOX_RML_ASSETS", native_assets, 1);
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
   const fs::path assets = fs::path(data) / "pad-choice-assets";
   fs::create_directories(assets);
   for (const auto& entry : fs::directory_iterator(native / "stage" / "megadrive"))
      if (entry.is_regular_file())
         fs::copy_file(entry.path(), assets / entry.path().filename(), fs::copy_options::overwrite_existing);
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
/* We mark the slot that SAVE and LOAD use only while one of them has focus.
 * With the pointer over a slot we highlight it without choosing it. A click
 * or an arrow key onto a slot chooses it. We check every registered design. */
bool slot_marked(int slot, int unmarked)
{
   const std::string id = "slot-" + std::to_string(slot);
   const std::string plain = "slot-" + std::to_string(unmarked);
   return std::string(inspect.property(id.c_str(), "border-top-color"))
         != inspect.property(plain.c_str(), "border-top-color");
}

/* The text of a button for a fact that the design placed in it. */
std::string fact_in(const char *button, const char *fact)
{
   Rml::Element *element = view.document.root()->GetElementById(button);
   Rml::Element *shown = element
         ? element->QuerySelector(std::string("[data-fact=") + fact + "]") : nullptr;
   return shown ? shown->GetInnerRML() : std::string();
}

bool buttons_name_slot(int slot)
{
   const std::string wanted = std::to_string(slot);
   return fact_in("save", "chosen-slot") == wanted && fact_in("load", "chosen-slot") == wanted;
}

void chosen_slot_shows_on_save_and_load(const char *native_assets)
{
   for (const char *design : {"native", "disc"})
   {
      const std::string assets = design_assets(native_assets, design);
      check(std::filesystem::is_regular_file(assets + "/menu.rml"), "the design is staged");
      setenv("ROMINABOX_RML_ASSETS", assets.c_str(), 1);
      host.slot_occupied = true;
      void *menu = open_menu();
      if (!menu) continue;
      const std::string name = design;
      const auto say = [&](const char *what) { return (name + ": " + what); };

      check(focused("resume") && !slot_marked(1, 6),
            say("the chosen slot is plain while CONTINUE has focus").c_str());
      check(buttons_name_slot(1), say("SAVE and LOAD name the chosen slot").c_str());
      hover_and_frame(menu, "slot-5");
      check(focused("slot-5") && slot_marked(5, 6) && !slot_marked(1, 6),
            say("the pointer over a slot highlights that slot").c_str());
      hover_and_frame(menu, "slot-2");
      check(focused("slot-2") && slot_marked(2, 6) && !slot_marked(5, 6),
            say("the highlight follows the pointer from slot to slot").c_str());
      hover_and_frame(menu, "save");
      check(slot_marked(1, 6) && !slot_marked(2, 6) && !slot_marked(5, 6),
            say("SAVE shows the chosen slot; passing over others did not choose them").c_str());
      check(buttons_name_slot(1), say("passing over slots does not change what SAVE and LOAD name").c_str());
      click_and_frame(menu, "slot-3");
      hover_and_frame(menu, "save");
      check(slot_marked(3, 6) && !slot_marked(1, 6),
            say("a clicked slot is the one SAVE shows").c_str());
      check(buttons_name_slot(3), say("SAVE and LOAD name a clicked slot").c_str());
      hover_and_frame(menu, "quit");
      check(!slot_marked(3, 6), say("leaving SAVE hides it again").c_str());
      click_and_frame(menu, "slot-1");
      hover_and_frame(menu, "load");
      check(focused("load") && slot_marked(1, 6) && !slot_marked(3, 6),
            say("LOAD shows the slot it loads").c_str());

      /* Keys only: after every press we mark the chosen slot exactly when
       * SAVE or LOAD has focus, and a slot reached by key is the chosen one. */
      hover_and_frame(menu, "resume");
      int chosen = 1;
      bool slot_then_save = false;
      bool on_slot = false;
      for (const rib_key key : {RIB_KEY_UP, RIB_KEY_LEFT, RIB_KEY_DOWN, RIB_KEY_RIGHT,
               RIB_KEY_UP, RIB_KEY_DOWN, RIB_KEY_RIGHT, RIB_KEY_DOWN})
      {
         rib_menu_key(menu, key);
         frame(menu);
         const std::string at = view.focus.current_id();
         if (at.rfind("slot-", 0) == 0)
         {
            chosen = std::atoi(at.c_str() + 5);
            on_slot = true;
         }
         check(buttons_name_slot(chosen), say("SAVE and LOAD name a slot reached by key").c_str());
         const bool aiming = at == "save" || at == "load";
         slot_then_save = slot_then_save || (on_slot && aiming);
         const int unmarked = chosen == 6 ? 5 : 6;
         for (int slot = 1; slot <= 6; ++slot)
            if ("slot-" + std::to_string(slot) != at && slot != unmarked)
               check(slot_marked(slot, unmarked) == (aiming && slot == chosen),
                     say("by keys, only SAVE and LOAD show the chosen slot").c_str());
      }
      check(slot_then_save, say("the keys reached a slot and then SAVE or LOAD").c_str());
      rib_menu_destroy(menu);
   }
   host.slot_occupied = false;
   setenv("ROMINABOX_RML_ASSETS", native_assets, 1);
}

void pad_changes_and_reset_apply_together(const char *native_assets, const char *data)
{
   const std::string assets = stage_pad_choice(native_assets, data);
   const std::string data_dir = std::string(data) + "/pad-choice-data";
   std::filesystem::create_directories(data_dir);
   setenv("ROMINABOX_RML_ASSETS", assets.c_str(), 1);
   setenv("ROMINABOX_DATA_DIR", data_dir.c_str(), 1);
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
   setenv("ROMINABOX_RML_ASSETS", native_assets, 1);
   setenv("ROMINABOX_DATA_DIR", data, 1);
}

/* The player decides in the game's Options, in every design, whether the game
 * keeps running while its window is in the background. We apply a change in
 * RetroArch at once and write it to the player's file. The switch shows the
 * current value in RetroArch, not the default from the export. */
void background_play_is_the_players(const char *native_assets, const char *data)
{
   const std::string file = std::string(data) + "/background-play.cfg";
   for (const char *design : {"native", "disc"})
   {
      const std::string assets = design_assets(native_assets, design);
      const std::string name = std::string(design) + ": ";
      setenv("ROMINABOX_RML_ASSETS", assets.c_str(), 1);
      std::remove(file.c_str());
      host.settings["pause_nonactive"] = 1.0f;
      void *menu = open_menu();
      if (!menu) continue;
      click_and_frame(menu, "options");
      check(view.document.has_element("background-play")
               && std::string(inspect.text("background-play-state")) == "OFF"
               && !inspect.has_class("background-play", "on"),
            (name + "Options offers PLAY IN BACKGROUND, off while RetroArch pauses in the background").c_str());
      click_and_frame(menu, "background-play");
      check(host.settings["pause_nonactive"] == 0.0f,
            (name + "turning it on stops RetroArch pausing in the background, at once").c_str());
      check(std::string(inspect.text("background-play-state")) == "ON"
               && inspect.has_class("background-play", "on"),
            (name + "the switch says ON and carries the fact `on`").c_str());
      check(read_file(file) == "pause_nonactive = \"false\"\n",
            (name + "the choice is written to the player's own file, as RetroArch reads it").c_str());
      hover_and_frame(menu, "background-play");
      rib_menu_key(menu, RIB_KEY_OK);
      frame(menu);
      check(focused("background-play") && host.settings["pause_nonactive"] == 1.0f
               && read_file(file) == "pause_nonactive = \"true\"\n"
               && std::string(inspect.text("background-play-state")) == "OFF",
            (name + "OK on the focused switch turns it back off, and that is written too").c_str());
      rib_menu_destroy(menu);
      /* The next launch starts from what the launcher applied, and the
       * shipped tests show that this is the player's file. */
      host.settings["pause_nonactive"] = 0.0f;
      if ((menu = open_menu()))
      {
         click_and_frame(menu, "options");
         check(std::string(inspect.text("background-play-state")) == "ON"
                  && inspect.has_class("background-play", "on"),
               (name + "the switch shows what RetroArch holds, not the export's default").c_str());
         rib_menu_destroy(menu);
      }
   }
   std::remove(file.c_str());
   setenv("ROMINABOX_RML_ASSETS", native_assets, 1);
}

/* A change of volume plays a cue at the chosen level, once per step. */
void volume_is_heard_at_its_level(const char *native_assets)
{
   /* The staged export has no sound pack, so it includes the tick. A game
    * with a pack has the same menu without it. */
   const std::string tick = std::string(native_assets) + "/volume-tick.wav";
   const std::string aside = tick + ".aside";
   check(std::ifstream(tick).good(), "an export with menu sounds off ships the volume tick");
   for (const bool pack : {true, false})
   {
      const char *which = pack ? "with a sound pack: " : "with menu sounds off: ";
      auto said = [&](const char *what) { return std::string(which) + what; };
      if (pack)
         std::rename(tick.c_str(), aside.c_str());
      else
         std::rename(aside.c_str(), tick.c_str());
      host.level_cue.clear();
      host.settings["audio_volume"] = 0.0f;
      void *menu = open_menu();
      if (!menu) continue;
      check(pack ? host.level_cue.empty() : host.level_cue == tick,
            said(pack ? "the pack's own cue plays, and no tick is loaded"
                      : "the game's own tick is loaded as the cue").c_str());
      click_and_frame(menu, "options");
      int x = 0, y = 0, w = 0, h = 0;
      check(view.document.element_box("volume-level", &x, &y, &w, &h) && w > 20,
            "the volume slider has a width to drag across");

      /* A drag from the top to the bottom, a pixel a frame. */
      host.sounds.clear();
      host.level_cue_db.clear();
      host.pointer.x = x + w - 1;
      host.pointer.y = y + h / 2;
      host.pointer.pressed = true;
      frame(menu);
      for (int at = x + w - 1; at >= x - 4; --at)
      {
         host.pointer.x = at;
         frame(menu);
      }
      host.pointer.pressed = false;
      frame(menu);
      bool falling = true;
      for (size_t index = 1; index < host.level_cue_db.size(); ++index)
         falling = falling && host.level_cue_db[index] < host.level_cue_db[index - 1];
      char message[256];
      std::snprintf(message, sizeof(message),
            "%sa drag from the top to the bottom is heard once a step: %zu cues for 9 steps",
            which, host.level_cue_db.size());
      check(host.level_cue_db.size() == 9 && falling, message);
      check(!host.level_cue_db.empty() && host.level_cue_db.back() == -80.0f
               && host.settings["audio_volume"] == -80.0f,
            said("each cue is asked for at the level just chosen, down to the bottom").c_str());
      check(std::all_of(host.sounds.begin(), host.sounds.end(),
                  [](rib::test::Sound sound) { return sound == rib::test::Sound::LevelDown; }),
            said("a drag down plays the level cue alone, not the move cue as well").c_str());

      /* An arrow: one cue, at the new level. Then the top, where it cannot move. */
      host.level_cue_db.clear();
      click_and_frame(menu, "volume-up");
      check(host.level_cue_db.size() == 1 && host.level_cue_db[0] > -80.0f
               && host.level_cue_db[0] == host.settings["audio_volume"],
            said("an arrow click is heard once, at the level it chose").c_str());
      for (int step = 0; step < 12; ++step)
         click_and_frame(menu, "volume-up");
      host.level_cue_db.clear();
      click_and_frame(menu, "volume-up");
      check(host.level_cue_db.empty() && host.settings["audio_volume"] == 0.0f,
            said("at the top an arrow changes nothing and plays nothing").c_str());
      rib_menu_destroy(menu);
   }
}

/* While nobody touches an open menu, we build no new geometry. RmlUi builds
 * geometry only for what a change makes it lay out or draw again. We add a
 * save state without telling the menu, which never happens in the player
 * (we announce a save when it finishes, and look again when the menu opens),
 * to show whether we keep requesting the slot files. */
void an_idle_menu_builds_nothing(const char *native_assets)
{
   setenv("ROMINABOX_RML_ASSETS", native_assets, 1);
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
 * few frames later. We show it in the open menu once it is there. */
void a_save_over_a_picture_shows_the_new_one(const char *data)
{
   const std::string picture = std::string(data) + "/resave-slot-1.png";
   std::ofstream(picture, std::ios::binary) << "the first save's picture";
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
   std::ofstream(picture, std::ios::binary | std::ios::trunc)
         << "the second save's picture, written after the report";
   for (int after = 0; after < 3; ++after)
      frame(menu);
   check(inspect.texture_loads() > before,
         "a save over a slot with a picture shows the new picture once RetroArch has "
         "written it, without the menu being reopened");
   rib_menu_destroy(menu);
   std::remove(picture.c_str());
   host.slot_occupied = false;
   host.thumbnail.clear();
}

/* A move that fails at once, for example onto a file open in another program. */
int failing_rename(const char *, const char *) { return -1; }

/* Every controls save after the first replaces the previous file, on every
 * platform, Windows included. On each platform we replace the file in one
 * step, and after a failed move the old file is still there. */
void repeated_saves_replace_the_file(const char *data)
{
   void *menu = open_menu();
   if (!menu) return;
   click_and_frame(menu, "options");
   click_and_frame(menu, "controls");
   for (int attempt = 1; attempt <= 2; ++attempt)
   {
      click_and_frame(menu, "controls-reset");
      check(std::string(inspect.text("controls-status")) == "DEFAULTS RESTORED",
            attempt == 1 ? "the first controls save succeeds"
                         : "a second controls save replaces the first");
   }
   const std::string volume = std::string(data) + "/b19-volume.cfg";
   check(rib_write_player_setting(volume.c_str(), "audio_volume", "-3.0")
            && rib_write_player_setting(volume.c_str(), "audio_volume", "-4.0"),
         "the volume file is replaced");
   rib_files_use_rename(failing_rename);
   check(!rib_write_player_setting(volume.c_str(), "audio_volume", "-5.0")
            && read_file(volume).find("-4.0") != std::string::npos
            && !std::ifstream(volume + ".tmp"),
         "a replace that fails keeps the old file and leaves no temporary");
   rib_files_use_rename(nullptr);
   rib_menu_destroy(menu);
}
}

int main(int argc, char **argv)
{
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
      rib_menu_destroy(menu);
   }

   fixes::an_idle_menu_builds_nothing(argv[1]);
   fixes::repeated_saves_replace_the_file(argv[2]);
   fixes::background_play_is_the_players(argv[1], argv[2]);
   fixes::volume_is_heard_at_its_level(argv[1]);
   fixes::design_prompt_survives_an_empty_status(argv[1], argv[2]);
   fixes::disc_list_keeps_its_page(argv[1], argv[2]);
   fixes::a_filter_row_applies_its_filter(argv[1]);
   fixes::binds_open_sooner_on_hover();
   fixes::pad_changes_and_reset_apply_together(argv[1], argv[2]);
   fixes::chosen_slot_shows_on_save_and_load(argv[1]);
   fixes::a_save_over_a_picture_shows_the_new_one(argv[2]);

   if (failures)
      std::fprintf(stderr, "%d menu orchestration failures\n", failures);
   return failures ? 1 : 0;
}
