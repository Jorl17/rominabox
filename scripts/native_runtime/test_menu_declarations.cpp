/* The production loader, with only the libretro config reader replaced. We
 * read every value whole, through config_get_entry. config_get_array, which
 * writes a truncated value AND returns false, is here for any code that
 * still reads a line into a buffer. */
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
extern "C" void RARCH_LOG(const char *, ...) {}
extern "C" void RARCH_WARN(const char *, ...) {}
extern "C" void RARCH_ERR(const char *, ...) {}

int test_menu_declarations()
{
   values = {
      {"screens", "pause skipped disc odd"}, {"screen_panel_pause", "pause-panel"},
      {"screen_panel_skipped", ""},
      {"screen_panel_disc", "disc-panel"}, {"screen_role_disc", "discs"},
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
      check(design.screens[1].role == rib::ScreenRole::Discs,
            "a role is read as the role it names");
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
   std::string profile = "megadrive";
   bool profile_present = false;
   auto *config = rib_open_controls("fixture", false, profile, &controls,
         &profile_present, [](const char *, unsigned *) { return false; });
   check(config && reads == 1, "a controls override is opened once");
   check(profile == std::string(64, 'p') && profile_present,
         "a long profile an override names is kept whole");
   config_file_free(config);
   values.clear();
   profile = "megadrive";
   config = rib_open_controls("fixture", false, profile, &controls,
         &profile_present, [](const char *, unsigned *) { return false; });
   check(config && profile == "megadrive" && !profile_present,
         "a file that names no pad leaves the pad as it was");
   config_file_free(config);

   /* Lists and ids longer than any buffer a line could be read into: a long
    * shader list must not read as no shaders at all, and a long list of one
    * pad's controls must not read as every control belonging to every pad. */
   {
      values.clear();
      std::string shader_ids;
      std::vector<std::string> shader_names;
      for (int index = 0; index < RIB_SHADER_MAX; ++index)
      {
         const std::string id = "a-shader-whose-id-is-longer-than-any-id-buffer-the-menu-had-number-"
               + std::to_string(index);
         shader_names.push_back(id);
         shader_ids += (index ? " " : "") + id;
         values["shader_preset_" + id] = "shaders/" + id + "/" + id + ".glslp";
      }
      values["shader_ids"] = shader_ids;
      rib_shader_catalog shaders{};
      rib_load_shaders("fixture", &shaders);
      check(shader_ids.size() > 1024 && shaders.count == RIB_SHADER_MAX,
            "a long list of shaders is read whole");
      check(shaders.count > 0 && std::string(shaders.entries[0].id) == shader_names[0]
               && std::string(shaders.entries[0].preset) == values["shader_preset_" + shader_names[0]],
            "a long shader id is kept whole, with its preset");
   }
   {
      values.clear();
      const std::string long_control = "a_control_whose_id_is_longer_than_thirty_two";
      values["controls_profile"] = "pad";
      values["rib_label_up"] = "UP";
      values["rib_label_x"] = "X";
      values["rib_label_" + long_control] = "LONG";
      values["rib_group_" + long_control] = "a_stick_group_whose_name_is_longer_than_thirty_two";
      std::string belonging = "up " + long_control;
      for (int index = 0; index < 100; ++index)
         belonging += " a_control_only_the_other_pads_have_" + std::to_string(index);
      values["controls_variant_controls_pad"] = belonging;
      std::string devices;
      std::vector<std::string> device_ids;
      for (int index = 0; index < RIB_DEVICE_MAX; ++index)
      {
         const std::string id = "a-controller-whose-id-is-longer-than-any-id-buffer-the-menu-had-"
               + std::to_string(index);
         device_ids.push_back(id);
         devices += (index ? " " : "") + id;
         values["controls_variant_name_" + id] = "Controller " + std::to_string(index);
      }
      values["controls_variants"] = devices;
      rib_controls_catalog pad{};
      std::string named;
      bool present = false;
      config_file_t *defaults = rib_open_controls("fixture", true, named, &pad, &present,
            [](const char *, unsigned *index) { *index = 0; return true; });
      bool up = false, other = false, long_one = false, long_group = false;
      for (int index = 0; index < pad.count; ++index)
      {
         const std::string id(pad.entries[index].id);
         up = up || id == "up";
         other = other || id == "x";
         long_one = long_one || id == long_control;
         long_group = long_group || std::string(pad.entries[index].group)
               == values["rib_group_" + long_control];
      }
      check(belonging.size() > 1024 && up && long_one && !other,
            "a long list of one pad's controls is read whole: its own controls, and not another pad's");
      check(long_group, "a long stick group name is kept whole");
      check(devices.size() > 512 && pad.device_count == RIB_DEVICE_MAX
               && std::string(pad.devices[0].id) == device_ids[0],
            "a long list of controllers with long ids is read whole");
      config_file_free(defaults);
   }
   if (!failures) std::puts("declaration load and existing field limits pass");
   return failures ? 1 : 0;
}
