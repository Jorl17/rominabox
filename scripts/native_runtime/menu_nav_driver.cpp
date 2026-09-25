/* Headless navigation driver: the menu code, a fake host and no window.
 *
 * Read cases from stdin, one directive per line, and run each one in a new
 * menu on a composed menu-assets directory:
 *
 *   case NAME             start a case
 *   assets DIR            the composed menu (menu.rml, design.cfg, ...)
 *   data DIR              the game's data directory, empty, made by the caller
 *   set KEY VALUE         host and service state before the menu opens:
 *                           discs N          disc images in the core
 *                           load 1           slot 1 contains a state
 *                           achievements S   signed-out | active | startup | failed
 *                           rows N           achievements in the list
 *                           pending 1        an earned achievement not uploaded
 *   ids ID...             ids of the case, reported when not in the document
 *   step STEP             in the menu script grammar:
 *                           key:NAME   up down left right ok select cancel
 *                                      start toggle resume, and tab (the
 *                                      physical key, through the text path)
 *                           hover:ID   move the pointer to the element's centre
 *                           press:ID   move the pointer there, then press and
 *                                      release it, one frame each, like a click
 *                           wait-ms:N  advance the clock
 *                           ID         click the element, as in a menu script
 *   run                   run the case
 *
 * After every step, record what the player sees highlighted (visible
 * elements with the `focused` class), what is marked as capturing a binding
 * (the `capturing` class), the visible screen panel and the requested
 * sounds. Print one JSON line per case for the caller to judge.
 *
 * We record sounds in the fake host and play nothing. */
#include "rmlui/menu_api.h"
#include "rmlui/host.h"
#include "rmlui_bridge.h"
#include "rmlui/view.hpp"
#include "rmlui/elements.hpp"
#include "menu_host_fake.h"
#include "../../vendor/retroarch/cheevos/rominabox.h"
#include <libretro.h>
#include "achievements_fake.hpp"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace {
using rib::test::host;

}

namespace {
struct Case
{
   std::string name, assets, data;
   std::vector<std::pair<std::string, std::string>> setup;
   std::vector<std::string> ids, steps;
};

std::string json(const std::string& text)
{
   std::string out = "\"";
   for (char c : text)
   {
      if (c == '"' || c == '\\') out += '\\';
      if (c == '\n') { out += "\\n"; continue; }
      out += c;
   }
   return out + "\"";
}

const char *sound_name(rib::test::Sound sound)
{
   switch (sound)
   {
      case rib::test::Sound::ScrollUp: return "move";
      case rib::test::Sound::ScrollDown: return "move";
      case rib::test::Sound::Ok: return "confirm";
      case rib::test::Sound::Cancel: return "cancel";
   }
   return "?";
}

/* The screen panels declared in the design's design.cfg. */
std::vector<std::string> panels(const std::string& assets)
{
   std::vector<std::string> found = {"pause-panel", "controls-panel"};
   std::ifstream cfg(assets + "/design.cfg");
   for (std::string line; std::getline(cfg, line); )
   {
      if (line.rfind("screen_panel_", 0) != 0) continue;
      const auto open = line.find('"');
      const auto close = line.rfind('"');
      if (open != std::string::npos && close > open)
         found.push_back(line.substr(open + 1, close - open - 1));
   }
   return found;
}

rib::View& view = rib::menu_view();

void frame(void *menu)
{
   host.clock_us += 16000;
   rib_menu_frame(menu, 960, 600);
}

/* The visible elements with a state class, as one id, a list, or null. */
std::string marked(const char *state)
{
   std::string ids = "[";
   bool first = true;
   rib::walk(view.document.root(), [&](Rml::Element *element) {
      if (rib::display_none(element)) return rib::Walk::SkipChildren;
      if (element->IsClassSet(state) && !element->IsClassSet("text-key")
            && !element->GetId().empty())
      {
         ids += (first ? "" : ",") + json(element->GetId());
         first = false;
      }
      return rib::Walk::Continue;
   });
   ids += "]";
   if (ids == "[]") return "null";
   if (ids.find(',') == std::string::npos) return ids.substr(1, ids.size() - 2);
   return ids;
}

std::string observe(const std::vector<std::string>& screen_panels, size_t& heard)
{
   std::string screen;
   for (const std::string& panel : screen_panels)
      if (auto *element = view.document.root()->GetElementById(panel))
         if (!rib::hidden(element)) { screen = panel; break; }
   std::string sounds = "[";
   for (size_t index = heard; index < host.sounds.size(); ++index)
      sounds += (index > heard ? "," : "") + json(sound_name(host.sounds[index]));
   sounds += "]";
   heard = host.sounds.size();
   return "{\"focused\":" + marked("focused") + ",\"capturing\":" + marked("capturing")
         + ",\"screen\":" + json(screen) + ",\"sounds\":" + sounds + "}";
}

bool step(void *menu, const std::string& text)
{
   static const struct { const char *name; rib_key key; } keys[] = {
      {"up", RIB_KEY_UP}, {"down", RIB_KEY_DOWN}, {"left", RIB_KEY_LEFT},
      {"right", RIB_KEY_RIGHT}, {"ok", RIB_KEY_OK}, {"select", RIB_KEY_SELECT},
      {"cancel", RIB_KEY_CANCEL}, {"start", RIB_KEY_START},
      {"toggle", RIB_KEY_TOGGLE}, {"resume", RIB_KEY_RESUME}};
   if (text.rfind("key:", 0) == 0)
   {
      const std::string name = text.substr(4);
      if (name == "tab")
      {
         rib_rmlui_text_event(true, RETROK_TAB, 0, 0);
         rib_rmlui_text_event(false, RETROK_TAB, 0, 0);
         frame(menu);
         return true;
      }
      for (const auto& key : keys)
         if (name == key.name)
         {
            rib_menu_key(menu, key.key);
            frame(menu);
            return true;
         }
      return false;
   }
   if (text.rfind("hover:", 0) == 0)
   {
      if (!view.document.element_center(text.substr(6).c_str(), &host.pointer.x, &host.pointer.y))
         return false;
      frame(menu);
      return true;
   }
   if (text.rfind("press:", 0) == 0)
   {
      if (!view.document.element_center(text.substr(6).c_str(), &host.pointer.x, &host.pointer.y))
         return false;
      host.pointer.pressed = true;
      frame(menu);
      host.pointer.pressed = false;
      frame(menu);
      return true;
   }
   if (text.rfind("wait-ms:", 0) == 0)
   {
      host.clock_us += std::atoll(text.c_str() + 8) * 1000;
      frame(menu);
      return true;
   }
   if (text.find(':') != std::string::npos || !view.document.click_element(text.c_str()))
      return false;
   frame(menu);
   return true;
}

/* For writing a table: every visible element the player can move the focus
 * to, with its box, after the last step of the case. */
void dump(const std::string& name)
{
   static const char *classes[] = {"menu-action", "slot", "list-row", "option-entry",
      "slider", "control-callout", "control-group", "control-picker-current",
      "control-picker-option", "account-input", "list-pager-prev", "list-pager-next"};
   std::fprintf(stderr, "== %s\n", name.c_str());
   rib::walk(view.document.root(), [&](Rml::Element *element) {
      if (rib::display_none(element)) return rib::Walk::SkipChildren;
      bool wanted = element->GetTagName() == "input";
      for (const char *name : classes) wanted = wanted || element->IsClassSet(name);
      if (wanted && !element->GetId().empty())
      {
         int x = 0, y = 0, w = 0, h = 0;
         view.document.element_box(element->GetId().c_str(), &x, &y, &w, &h);
         std::fprintf(stderr, "   %-34s %4d,%4d %4dx%-4d%s\n", element->GetId().c_str(),
               x, y, w, h, element->HasAttribute("disabled") ? " disabled" : "");
      }
      return rib::Walk::Continue;
   });
}

void reset_services(const Case& run)
{
   host = rib::test::FakeHost{};
   host.pointer.x = 1;
   host.pointer.y = 1;
   host.clock_us = 1000000;
   session = {};
   session.status = RIB_ACHIEVEMENTS_SIGNED_OUT;
   service_rows.clear();
   for (const auto& [key, value] : run.setup)
   {
      if (key == "discs") host.disc_count = (unsigned)std::atoi(value.c_str());
      else if (key == "load") host.slot_occupied = value == "1";
      else if (key == "pending") session.pending_upload = value == "1";
      else if (key == "achievements")
      {
         if (value == "active")
         {
            session.status = RIB_ACHIEVEMENTS_ACTIVE;
            std::snprintf(session.account, sizeof(session.account), "player");
         }
         else if (value == "startup")
            session.startup_waiting = true;
         else if (value == "failed")
         {
            session.status = RIB_ACHIEVEMENTS_ERROR;
            std::snprintf(session.account, sizeof(session.account), "player");
         }
      }
      else if (key == "rows")
         for (int index = 0; index < std::atoi(value.c_str()); ++index)
         {
            rib_achievement_row_t row{};
            row.id = (uint32_t)index + 1;
            row.points = 5;
            std::snprintf(row.title, sizeof(row.title), "Achievement %d", index + 1);
            std::snprintf(row.description, sizeof(row.description), "Do thing %d", index + 1);
            service_rows.push_back(row);
         }
      else
      {
         std::fprintf(stderr, "%s: unknown setup %s\n", run.name.c_str(), key.c_str());
         std::exit(2);
      }
   }
   session.count = service_rows.size();
   ++session.revision;
}

void run_case(const Case& run)
{
   reset_services(run);
   setenv("ROMINABOX_RML_ASSETS", run.assets.c_str(), 1);
   setenv("ROMINABOX_DATA_DIR", run.data.c_str(), 1);
   unsetenv("ROMINABOX_MENU_SCRIPT");
   void *menu = rib_menu_create();
   if (!menu)
   {
      std::fprintf(stderr, "%s: no menu\n", run.name.c_str());
      std::exit(1);
   }
   frame(menu);
   rib_menu_toggle(menu, true);
   frame(menu);
   if (!view.document.root())
   {
      std::fprintf(stderr, "%s: %s did not load\n", run.name.c_str(), run.assets.c_str());
      std::exit(1);
   }
   std::string missing = "[";
   for (const std::string& id : run.ids)
      if (!view.document.has_element(id.c_str()))
         missing += (missing.size() > 1 ? "," : "") + json(id);
   missing += "]";
   const auto screen_panels = panels(run.assets);
   size_t heard = host.sounds.size();
   std::string steps = "[";
   if (missing == "[]")
      for (const std::string& text : run.steps)
      {
         if (!step(menu, text))
         {
            std::fprintf(stderr, "%s: cannot run step '%s'\n", run.name.c_str(), text.c_str());
            std::exit(1);
         }
         steps += (steps.size() > 1 ? "," : "") + observe(screen_panels, heard);
      }
   steps += "]";
   if (getenv("ROMINABOX_NAVIGATION_DUMP")) dump(run.name);
   rib_menu_destroy(menu);
   std::printf("{\"case\":%s,\"missing\":%s,\"steps\":%s}\n",
         json(run.name).c_str(), missing.c_str(), steps.c_str());
   std::fflush(stdout);
}
}

int main()
{
   Case current;
   int cases = 0;
   for (std::string line; std::getline(std::cin, line); )
   {
      if (line.empty()) continue;
      const auto space = line.find(' ');
      const std::string word = line.substr(0, space);
      const std::string rest = space == std::string::npos ? "" : line.substr(space + 1);
      if (word == "case") current = Case{rest};
      else if (word == "assets") current.assets = rest;
      else if (word == "data") current.data = rest;
      else if (word == "set")
      {
         const auto split = rest.find(' ');
         current.setup.emplace_back(rest.substr(0, split),
               split == std::string::npos ? "" : rest.substr(split + 1));
      }
      else if (word == "ids")
      {
         std::istringstream words(rest);
         for (std::string id; words >> id; ) current.ids.push_back(id);
      }
      else if (word == "step") current.steps.push_back(rest);
      else if (word == "run") { run_case(current); ++cases; }
      else
      {
         std::fprintf(stderr, "unknown directive: %s\n", line.c_str());
         return 2;
      }
   }
   std::fprintf(stderr, "menu_nav_driver: %d cases\n", cases);
   return cases ? 0 : 1;
}
