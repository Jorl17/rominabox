/* The chosen save slot through the production menu, its document and the
 * designs: marked whatever has focus, named on SAVE and LOAD, chosen by a
 * click or OK, and only highlighted by the pointer or the arrows. Part of
 * test_menu_orchestration, where main runs it with the other cases. */
#include "test_menu_orchestration.hpp"
#include "rmlui_bridge.h"
#include "test_environment.h"
#include <streams/file_stream.h>

#include <filesystem>
#include <string>

namespace fixes {
/* We mark the slot that SAVE and LOAD use whatever has focus, and not in the
 * same way as focus. The pointer or an arrow key onto a slot highlights it
 * without choosing it, and a click or OK chooses it. We check every
 * registered design against a plain slot, neither highlighted nor chosen. */
bool unlike_plain(const std::string& part, const char *property, int slot, int plain)
{
   return std::string(inspect.property((part + std::to_string(slot)).c_str(), property))
         != inspect.property((part + std::to_string(plain)).c_str(), property);
}
bool slot_highlighted(int slot, int plain) { return unlike_plain("slot-", "border-top-color", slot, plain); }
/* The colour of the words of the chosen slot in the design, read from slot 1
 * as the menu opens with CONTINUE focused. Focus takes precedence: we show the
 * focused slot as focused, chosen or not, so we read the mark on others. */
std::string chosen_words;
bool slot_chosen(int slot, int plain)
{
   const std::string words = inspect.property(("slot-label-" + std::to_string(slot)).c_str(), "color");
   return unlike_plain("slot-label-", "color", slot, plain) && words == chosen_words;
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

/* A game opens on the slot that the player chose last, so each design is a
 * separate game with a new data folder, and it opens on slot 1. */
void chosen_slot_shows_on_save_and_load(const char *native_assets, const char *data)
{
   for (const char *design : {"native", "disc"})
   {
      const std::string assets = design_assets(native_assets, design);
      check(std::filesystem::is_regular_file(assets + "/menu.rml"), "the design is staged");
      const std::string data_dir = std::string(data) + "/chosen-slot-" + design;
      std::filesystem::create_directories(data_dir);
      test_setenv("ROMINABOX_RML_ASSETS", assets.c_str());
      test_setenv("ROMINABOX_DATA_DIR", data_dir.c_str());
      host.slot_occupied = true;
      void *menu = open_menu();
      if (!menu) continue;
      const std::string name = design;
      const auto say = [&](const char *what) { return (name + ": " + what); };

      chosen_words = inspect.property("slot-label-1", "color");
      check(focused("resume") && slot_chosen(1, 6) && !slot_highlighted(1, 6),
            say("the chosen slot is marked, not highlighted, while CONTINUE has focus").c_str());
      check(buttons_name_slot(1), say("SAVE and LOAD name the chosen slot").c_str());
      hover_and_frame(menu, "slot-5");
      check(focused("slot-5") && slot_highlighted(5, 6) && !slot_chosen(5, 6) && slot_chosen(1, 6),
            say("the pointer over a slot highlights it without choosing it").c_str());
      hover_and_frame(menu, "slot-2");
      check(focused("slot-2") && slot_highlighted(2, 6) && !slot_highlighted(5, 6),
            say("the highlight follows the pointer from slot to slot").c_str());
      hover_and_frame(menu, "save");
      check(slot_chosen(1, 6) && !slot_chosen(2, 6) && !slot_chosen(5, 6),
            say("passing over slots did not choose them").c_str());
      check(buttons_name_slot(1), say("passing over slots does not change what SAVE and LOAD name").c_str());
      click_and_frame(menu, "slot-3");
      view.focus.set("resume");
      frame(menu);
      check(slot_chosen(3, 6) && !slot_chosen(1, 6), say("a clicked slot is the chosen one").c_str());
      check(buttons_name_slot(3), say("SAVE and LOAD name a clicked slot").c_str());
      click_and_frame(menu, "slot-1");

      /* Keys only: after every press slot 1 stays the chosen one, and only
       * the slot with focus is highlighted. */
      hover_and_frame(menu, "resume");
      bool slot_then_save = false;
      bool on_slot = false;
      for (const rib_key key : {RIB_KEY_UP, RIB_KEY_LEFT, RIB_KEY_DOWN, RIB_KEY_RIGHT,
               RIB_KEY_UP, RIB_KEY_DOWN, RIB_KEY_RIGHT, RIB_KEY_DOWN})
      {
         rib_menu_key(menu, key);
         frame(menu);
         const std::string at = view.focus.current_id();
         on_slot = on_slot || at.rfind("slot-", 0) == 0;
         check(buttons_name_slot(1), say("an arrow onto a slot does not change what SAVE and LOAD name").c_str());
         slot_then_save = slot_then_save || (on_slot && (at == "save" || at == "load"));
         const int plain = at == "slot-6" ? 5 : 6;
         for (int slot = 1; slot <= 6; ++slot)
            if (slot != plain)
            {
               if ("slot-" + std::to_string(slot) != at)
                  check(slot_chosen(slot, plain) == (slot == 1),
                        say("by keys, the chosen slot stays marked and no other is").c_str());
               check(slot_highlighted(slot, plain) == ("slot-" + std::to_string(slot) == at),
                     say("by keys, only the slot with focus is highlighted").c_str());
            }
      }
      check(slot_then_save, say("the keys reached a slot and then SAVE or LOAD").c_str());
      view.focus.set("slot-4");
      rib_menu_key(menu, RIB_KEY_OK);
      frame(menu);
      check(focused("slot-4") && !slot_chosen(1, 6) && buttons_name_slot(4),
            say("OK on a slot chooses it").c_str());
      view.focus.set("resume");
      frame(menu);
      check(slot_chosen(4, 6), say("the slot OK chose is marked once focus moves on").c_str());
      rib_menu_destroy(menu);
   }
   host.slot_occupied = false;
   test_setenv("ROMINABOX_RML_ASSETS", native_assets);
   test_setenv("ROMINABOX_DATA_DIR", data);
}

/* A game that resumes its autosave and opens at its menu before its first
 * frame has no frame of that position either. For a save then, we copy the
 * picture of the autosave, which RetroArch numbers -1. */
void a_save_before_the_game_runs_gets_the_autosave_picture(const char *data)
{
   const std::string picture = std::string(data) + "/autosave.png";
   const std::string data_dir = std::string(data) + "/autosave-picture-data";
   std::filesystem::create_directories(data_dir);
   test_setenv("ROMINABOX_DATA_DIR", data_dir.c_str());
   const std::string words = "the picture of the autosave";
   filestream_write_file(picture.c_str(), words.data(), (int64_t)words.size());
   host.game_has_run = false;
   host.resumed_autosave = true;
   host.autosave_thumbnail = picture;
   host.save_accepted = true;
   host.picture_copies.clear();
   void *menu = open_menu();
   if (!menu) return;
   click_and_frame(menu, "slot-2");
   click_and_frame(menu, "save");
   const bool copied_picture = rib_rmlui_notify_state_task("", 2, true, true);
   frame(menu);
   check(host.picture_copies == std::vector<std::pair<int, int>>{{-1, 2}},
         "a save before the game has run gets the picture of the autosave it resumed");
   check(copied_picture, "we take no picture of the game for that save");
   rib_menu_destroy(menu);
   std::remove(picture.c_str());
   host.game_has_run = true;
   host.resumed_autosave = false;
   host.autosave_thumbnail.clear();
   host.save_accepted = false;
   test_setenv("ROMINABOX_DATA_DIR", data);
}
}
