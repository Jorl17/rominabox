/* The production loader, with only the RetroArch file and config boundary
 * replaced. config_get_array writes a truncated value AND returns false.
 * Keep the separate handling of optional screen fields and toggle words. */
#include "rmlui/declarations.h"
#include <file/config_file.h>
#include <map>
#include <string>
#include <cstdio>
#include <cstring>
#include <cstdlib>

static std::map<std::string, std::string> values;
static unsigned reads;
extern "C" config_file_t *config_file_new_from_path_to_string(const char *)
{
   ++reads;
   return reinterpret_cast<config_file_t *>(&values);
}
extern "C" void config_file_free(config_file_t *) {}
extern "C" bool config_get_array(config_file_t *, const char *key, char *out, size_t size)
{
   auto found = values.find(key);
   return found != values.end() && strlcpy(out, found->second.c_str(), size) < size;
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
      {"screens", "pause skipped disc"}, {"screen_panel_pause", "pause-panel"},
      {"screen_panel_skipped", std::string(128, 'p')},
      {"screen_panel_disc", "disc-panel"}, {"screen_images_disc", "list"},
      {"screen_mark_disc", std::string(32, 'm')},
      {"screen_heading_pause", std::string(128, 'h')},
      {"screen_footer_pause", std::string(128, 'f')},
      {"screen_button_pause", std::string(128, 'b')},
      {"screen_images_pause", std::string(64, 'i')},
      {"overlays", "logo"}, {"overlay_hold_logo", "1000"},
      {"overlay_follows_logo", std::string(64, 'f')},
      {"overlay_needs_logo", std::string(128, 'n')},
      {"binds_list", std::string(64, 'b')},
      {"toggles", "mode"}, {"toggle_on_mode", std::string(32, 't')},
   };
   reads = 0;
   auto *loaded = rib_load_design("fixture");
   const auto *data = rib_design_get(loaded);
   int failures = 0;
   auto check = [&](bool ok, const char *what) {
      if (!ok) { std::fprintf(stderr, "FAIL declarations: %s\n", what); ++failures; }
   };
   check(reads == 1, "design.cfg is opened once");
   check(data->screen_count == 2, "an oversized required panel omits that screen");
   if (data->screen_count == 2)
   {
      check(!data->screens[0].heading[0], "oversized heading is empty");
      check(!data->screens[0].footer[0], "oversized footer is empty");
      check(!data->screens[0].button[0], "oversized button is empty");
      check(!data->screens[0].images[0], "oversized images redirect is empty");
      check(!data->screens[1].mark[0], "oversized disc mark is empty");
   }
   check(data->overlay_count == 1, "oversized optional needs does not omit the overlay");
   if (data->overlay_count == 1)
   {
      check(!data->overlays[0].follows[0], "oversized follows is empty");
      check(!data->overlays[0].needs[0], "oversized needs is empty");
   }
   check(!data->binds_list[0], "oversized binds list is empty");
   check(data->toggle_count == 1 && std::strlen(data->toggles[0].on) == 31,
         "toggle words retain their existing truncation policy");
   rib_design_free(loaded);
   if (!failures) std::puts("declaration load and existing field limits pass");
   return failures ? 1 : 0;
}
