/* The player's settings through the production menu, its document and file
 * layer: volume, background play, rumble, and the files that store them.
 * Part of test_menu_orchestration, where main runs these cases in order with
 * the others. Only the RetroArch host commands are fake. */
#include "test_menu_orchestration.hpp"
#include "rmlui/files.h"
#include "test_environment.h"
#include <file/config_file.h>
#include <file/file_path.h>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>

namespace fixes {
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
      test_setenv("ROMINABOX_RML_ASSETS", assets.c_str());
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
   test_setenv("ROMINABOX_RML_ASSETS", native_assets);
}

/* The player can turn rumble off in the game's Options, in every design, but
 * only in a game whose core requested rumble on a pad. In any other game we
 * disable the setting and hide it. We apply a change in RetroArch at once and
 * write it to the player's file. */
/* Options, turned to the page with `id`. A design splits its entries into
 * pages, and the platform screen (UNINSTALL or RESET) can move one onward. */
void show_option(void *menu, const char *id)
{
   int x = 0, y = 0, w = 0, h = 0;
   for (int turns = 0; turns < 4 && !inspect.box(id, &x, &y, &w, &h); ++turns)
      click_and_frame(menu, "options-next");
}

void rumble_is_the_players_where_the_game_rumbles(const char *native_assets, const char *data)
{
   const std::string file = std::string(data) + "/rumble.cfg";
   int x = 0, y = 0, w = 0, h = 0;
   for (const char *design : {"native", "disc"})
   {
      const std::string assets = design_assets(native_assets, design);
      const std::string name = std::string(design) + ": ";
      test_setenv("ROMINABOX_RML_ASSETS", assets.c_str());
      std::remove(file.c_str());
      host.settings["input_rumble_enable"] = 1.0f;
      host.rumbles = false;
      void *menu = open_menu();
      if (!menu) continue;
      click_and_frame(menu, "options");
      check(inspect.has_class("rumble", "disabled") && !inspect.box("rumble", &x, &y, &w, &h),
            (name + "a game whose core never asked to rumble has RUMBLE disabled, and not shown").c_str());
      rib_menu_destroy(menu);

      host.rumbles = true;
      if (!(menu = open_menu())) continue;
      click_and_frame(menu, "options");
      show_option(menu, "rumble");
      check(!inspect.has_class("rumble", "disabled") && inspect.box("rumble", &x, &y, &w, &h)
               && std::string(inspect.text("rumble-state")) == "ON"
               && inspect.has_class("rumble", "on"),
            (name + "a game that rumbles shows RUMBLE, on").c_str());
      click_and_frame(menu, "rumble");
      check(host.settings["input_rumble_enable"] == 0.0f
               && std::string(inspect.text("rumble-state")) == "OFF"
               && !inspect.has_class("rumble", "on"),
            (name + "turning it off turns RetroArch's rumble off at once, and says OFF").c_str());
      check(read_file(file) == "input_rumble_enable = \"false\"\n",
            (name + "the choice is written to the player's own file, as RetroArch reads it").c_str());
      hover_and_frame(menu, "rumble");
      rib_menu_key(menu, RIB_KEY_OK);
      frame(menu);
      check(focused("rumble") && host.settings["input_rumble_enable"] == 1.0f
               && read_file(file) == "input_rumble_enable = \"true\"\n"
               && std::string(inspect.text("rumble-state")) == "ON",
            (name + "OK on the focused switch turns it back on, and that is written too").c_str());
      rib_menu_destroy(menu);
   }
   host.rumbles = false;
   std::remove(file.c_str());
   test_setenv("ROMINABOX_RML_ASSETS", native_assets);
}

/* A change of volume plays a cue at the chosen level, once per step. The
 * bottom step is silence, so we request no cue there, whichever host plays
 * the cues. */
void volume_is_heard_at_its_level(const char *native_assets)
{
   /* The staged export has no sound pack, so it includes the tick. A game
    * with a pack has the same menu without it. */
   const std::string tick = std::string(native_assets) + "/volume-tick.wav";
   const std::string aside = tick + ".aside";
   check(path_is_valid(tick.c_str()), "an export with menu sounds off ships the volume tick");
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
            "%sa drag from the top to the bottom is heard once a step but the silent last: "
            "%zu cues for 9 steps", which, host.level_cue_db.size());
      check(host.level_cue_db.size() == 8 && falling, message);
      check(!host.level_cue_db.empty() && host.level_cue_db.back() > -80.0f
               && host.settings["audio_volume"] == -80.0f,
            said("each cue is asked for at the level just chosen, and none at the bottom").c_str());
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

/* Replace the player's volume file and count each replacement. */
int volume_writes;
int counting_rename(const char *from, const char *to)
{
   const std::string target(to);
   const std::string name = "/volume.cfg";
   if (target.size() >= name.size()
         && target.compare(target.size() - name.size(), name.size(), name) == 0)
      ++volume_writes;
   return std::rename(from, to);
}

/* Loading the menu is not a change. We write nothing for a level at a
 * position, as applied at launch from the two decimals in the file, and we
 * move a level between two positions, from a hotkey or a file written with
 * other steps, to the nearest one and store that. */
void a_menu_load_writes_the_volume_only_off_a_position()
{
   rib_files_use_rename(counting_rename);
   struct Load { float level; int writes; float lands; const char *what; };
   for (const Load& load : {
            Load{-26.1f, 0, -26.1f, "a level at a position, as its file holds it, is not written on a menu load"},
            Load{-38.2f, 0, -38.2f, "nor is one a step above the bottom"},
            Load{-44.4f, 1, -38.2f, "a level between positions is put on the nearest and written once"},
            Load{-8.9f, 1, -10.2f, "the nearest is by decibels, not by the old even steps"}})
   {
      host.settings["audio_volume"] = load.level;
      volume_writes = 0;
      void *menu = open_menu();
      if (!menu) continue;
      for (int settle = 0; settle < 3; ++settle)
         frame(menu);
      char message[256];
      std::snprintf(message, sizeof(message), "%s: %d writes at %.1f dB, now %.1f dB",
            load.what, volume_writes, load.level, host.settings["audio_volume"]);
      check(volume_writes == load.writes && host.settings["audio_volume"] == load.lands,
            message);
      rib_menu_destroy(menu);
   }
   rib_files_use_rename(nullptr);
}

/* A drag that the player is still making when the menu closes, as with
 * Escape or the menu button of a pad during the drag. We applied each step
 * as the drag passed it, and we keep the level it reached as for a released
 * drag, so the next launch starts from it. */
void a_drag_cut_short_by_closing_is_kept(const char *data)
{
   const std::string file = std::string(data) + "/volume.cfg";
   std::remove(file.c_str());
   host.settings["audio_volume"] = 0.0f;
   host.pointer = {};
   void *menu = open_menu();
   if (!menu) return;
   click_and_frame(menu, "options");
   int x = 0, y = 0, w = 0, h = 0;
   check(view.document.element_box("volume-level", &x, &y, &w, &h) && w > 20,
         "the volume slider has a width to drag across");
   host.pointer.x = x + w - 1;
   host.pointer.y = y + h / 2;
   host.pointer.pressed = true;
   frame(menu);
   for (int at = x + w - 1; at >= x + w / 2; at -= 4)
   {
      host.pointer.x = at;
      frame(menu);
   }
   const float dragged = host.settings["audio_volume"];
   check(dragged < 0.0f && dragged > -80.0f, "the drag moved the level part of the way");
   /* The menu closes with the button still held. */
   rib_menu_toggle(menu, false);
   host.menu_open = false;
   frame(menu);
   frame(menu);
   rib_menu_destroy(menu);

   /* The next launch: the launcher applies the player's file. */
   config_file_t *saved = config_file_new_from_path_to_string(file.c_str());
   float level = 1.0f;
   check(saved && config_get_float(saved, "audio_volume", &level)
            && std::fabs(level - dragged) < 0.06f,
         "a drag cut short by closing the menu is written to the player's file");
   if (saved) config_file_free(saved);
   host.settings["audio_volume"] = level;
   host.menu_open = true;
   host.pointer = {};
   if ((menu = open_menu()))
   {
      click_and_frame(menu, "options");
      const auto& fractions = view.parts.fractions();
      const auto shown = fractions.find("volume-level");
      check(level != 1.0f && shown != fractions.end() && shown->second > 0.0f
               && shown->second < 1.0f,
            "the relaunched menu shows the level the cut-short drag reached");
      rib_menu_destroy(menu);
   }
   std::remove(file.c_str());
}

/* Each step is a similar change to the ear, so five steps up from silence
 * is clearly audible, about -10 dB, rather than the -36 dB that equal
 * decibel steps reach. We test this through the Options composed in the
 * export and the arrows that a player clicks. */
void the_middle_of_the_volume_is_clearly_audible()
{
   host.settings["audio_volume"] = -80.0f;
   void *menu = open_menu();
   if (!menu) return;
   click_and_frame(menu, "options");
   for (int step = 0; step < 5; ++step)
      click_and_frame(menu, "volume-up");
   const float level = host.settings["audio_volume"];
   char message[160];
   std::snprintf(message, sizeof(message),
         "five steps up from silence is about -10 dB, got %.1f dB", level);
   check(level > -11.0f && level < -9.5f, message);
   rib_menu_destroy(menu);
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
            && !path_is_valid((volume + ".tmp").c_str()),
         "a replace that fails keeps the old file and leaves no temporary");
   rib_files_use_rename(nullptr);
   rib_menu_destroy(menu);
}
}
