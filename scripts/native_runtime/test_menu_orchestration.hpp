/* Code shared by the sources of test_menu_orchestration: the menu under
 * test, its inspection and fake host, the failure count, and the steps of
 * every case. test_menu_orchestration.cpp contains main and the menu cases,
 * test_menu_player_settings.cpp the cases for the player's settings, and
 * test_menu_slots.cpp the cases for the chosen save slot. */
#pragma once
#include "rmlui/menu_api.h"
#include "rmlui/view.hpp"
#include "menu_test_view.hpp"
#include "menu_host_fake.h"
#include <streams/file_stream.h>

#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <string>

inline rib::View& view = rib::menu_view();
inline rib::test::Inspection inspect(view.document);
using rib::test::host;
inline int failures;

/* The text of a file, read through the libretro file layer as in the menu,
 * with UTF-8 paths on every platform. Empty when there is no such file. */
inline std::string read_file(const std::filesystem::path& path)
{
   void *bytes = nullptr;
   int64_t size = 0;
   if (!filestream_read_file(path.u8string().c_str(), &bytes, &size))
      return std::string();
   std::string text(static_cast<const char*>(bytes), (size_t)size);
   free(bytes);
   return text;
}

inline void check(bool condition, const char *message)
{
   if (!condition)
   {
      std::fprintf(stderr, "FAIL menu orchestration: %s\n", message);
      ++failures;
   }
}

inline void frame(void *menu) { rib::test::loop_pass(menu, 960, 600); }

inline void click_and_frame(void *menu, const char *id)
{
   check(view.document.click_element(id), id);
   frame(menu);
}

inline void hover_and_frame(void *menu, const char *id)
{
   check(view.document.element_center(id, &host.pointer.x, &host.pointer.y), id);
   frame(menu);
}

inline bool focused(const char *id)
{
   return inspect.has_class(id, "focused");
}

namespace fixes {
inline void *open_menu()
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
inline std::string design_assets(const char *native_assets, const char *design)
{
   return (std::filesystem::path(native_assets).parent_path() / (std::string("placement-") + design)).string();
}

/* The chosen save slot, in test_menu_slots.cpp. */
void chosen_slot_shows_on_save_and_load(const char *native_assets, const char *data);
/* The player's settings, in test_menu_player_settings.cpp. */
void background_play_is_the_players(const char *native_assets, const char *data);
void rumble_is_the_players_where_the_game_rumbles(const char *native_assets, const char *data);
void volume_is_heard_at_its_level(const char *native_assets);
void a_menu_load_writes_the_volume_only_off_a_position();
void a_drag_cut_short_by_closing_is_kept(const char *data);
void the_middle_of_the_volume_is_clearly_audible();
void repeated_saves_replace_the_file(const char *data);
}
