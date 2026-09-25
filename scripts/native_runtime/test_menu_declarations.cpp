/* The production loader, with only the RetroArch file and config boundary
 * replaced. config_get_array writes a truncated value AND returns false.
 * Keep the separate handling of optional screen fields and toggle words. */
#include "rmlui/declarations.h"
#include <file/config_file.h>
#include <map>
#include <vector>
#include <string>
#include <cstdio>
#include <cstring>
#include <cstdlib>

static std::map<std::string, std::string> values;
static unsigned reads;
static std::vector<config_entry_list> entries;
extern "C" config_file_t *config_file_new_from_path_to_string(const char *)
{
   ++reads;
   entries.clear();
   for (auto& value : values)
      entries.push_back({const_cast<char*>(value.first.c_str()),
            const_cast<char*>(value.second.c_str()), nullptr, false});
   for (size_t index = 1; index < entries.size(); ++index)
      entries[index - 1].next = &entries[index];
   return reinterpret_cast<config_file_t *>(&values);
}
extern "C" void config_file_free(config_file_t *) {}
extern "C" bool config_get_array(config_file_t *, const char *key, char *out, size_t size)
{
   auto found = values.find(key);
   return found != values.end() && strlcpy(out, found->second.c_str(), size) < size;
}
extern "C" config_entry_list *config_get_entry(const config_file_t *, const char *key)
{
   for (auto& entry : entries)
      if (std::strcmp(entry.key, key) == 0) return &entry;
   return nullptr;
}
extern "C" bool config_get_entry_list_next(config_file_entry *entry)
{
   if (!entry->next) return false;
   const auto *current = entry->next;
   *entry = {current->key, current->value, current->next};
   return true;
}
extern "C" bool config_get_entry_list_head(config_file_t *, config_file_entry *entry)
{
   entry->next = entries.empty() ? nullptr : &entries[0];
   return config_get_entry_list_next(entry);
}
extern "C" bool config_get_int(config_file_t *, const char *key, int *out)
{
   auto found = values.find(key);
   if (found == values.end()) return false;
   *out = std::atoi(found->second.c_str());
   return true;
}
extern "C" bool path_is_valid(const char *) { return false; }
extern "C" void RARCH_LOG(const char *, ...) {}
extern "C" void RARCH_WARN(const char *, ...) {}
extern "C" void RARCH_ERR(const char *, ...) {}

int test_menu_declarations()
{
   values = {
      {"screens", "pause skipped disc odd"}, {"screen_panel_pause", "pause-panel"},
      {"screen_panel_skipped", ""},
      {"screen_panel_disc", "disc-panel"}, {"screen_role_disc", "discs"},
      {"screen_mark_disc", std::string(32, 'm')},
      {"screen_panel_odd", "odd-panel"}, {"screen_role_odd", "nothing-known"},
      {"screen_heading_pause", std::string(128, 'h')},
      {"screen_footer_pause", std::string(128, 'f')},
      {"screen_button_pause", " one  two "},
      {"screen_images_pause", std::string(64, 'i')},
      {"overlays", "logo"}, {"overlay_hold_logo", "1000"},
      {"overlay_follows_logo", std::string(64, 'f')},
      {"binds_list", std::string(64, 'b')},
      {"fonts", "One.ttf Two.ttf"},
   };
   reads = 0;
   const rib::DesignDeclarations design = rib::load_design("fixture");
   int failures = 0;
   auto check = [&](bool ok, const char *what) {
      if (!ok) { std::fprintf(stderr, "FAIL declarations: %s\n", what); ++failures; }
   };
   check(reads == 1, "design.cfg is opened once");
   check(design.screens.size() == 3, "a screen without a panel is left out");
   if (design.screens.size() == 3)
   {
      const rib::ScreenDeclaration& pause = design.screens[0];
      check(pause.heading == std::string(128, 'h') && pause.footer == std::string(128, 'f')
            && pause.images == std::string(64, 'i'),
            "a long heading, footer and redirect are kept whole");
      check(pause.buttons == std::vector<std::string>{"one", "two"},
            "a screen's buttons are read once, as a list");
      check(pause.role == rib::ScreenRole::None, "a screen that declares no role has none");
      check(design.screens[1].role == rib::ScreenRole::Discs
            && design.screens[1].mark == std::string(32, 'm'),
            "a role is read as the role it names, and a long mark is kept whole");
      check(design.screens[2].role == rib::ScreenRole::None,
            "a role this player does not know is no role");
   }
   check(design.overlays.size() == 1 && design.overlays[0].follows == std::string(64, 'f'),
         "a long follows is kept whole");
   check(design.binds.list == std::string(64, 'b'), "a long bind list id is kept whole");
   check(design.fonts == std::vector<std::string>{"One.ttf", "Two.ttf"},
         "the fonts are the files design.cfg lists, in order");
   values = {{"controls_profile", std::string(64, 'p')}};
   reads = 0;
   rib_controls_catalog controls{};
   char profile[32] = "megadrive";
   bool profile_present = false;
   auto *config = rib_open_controls("fixture", false, profile, &controls,
         &profile_present, [](const char *, unsigned *) { return false; });
   check(config && reads == 1, "a controls override is opened once");
   check(std::strcmp(profile, "megadrive") == 0 && profile_present,
         "a truncated override keeps the profile but still repaints the picker");
   config_file_free(config);
   if (!failures) std::puts("declaration load and existing field limits pass");
   return failures ? 1 : 0;
}
