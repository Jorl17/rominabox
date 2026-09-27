/* A headless runner for the menu scripts of a launched player, with no window.
 *
 * In the exported player, we run ROMINABOX_MENU_SCRIPT through the script
 * driver in the fork and print a checkpoint for each `report:` step. Here we
 * run the same scripts through the same driver and report code, in the
 * production menu, with the fake RetroArch host and a fake clock, many cases
 * in one process, with nothing drawn or played.
 *
 * We read cases from stdin, one directive per line:
 *
 *   case NAME           starts a case
 *   assets DIR          the composed menu, as an export stages menu-assets
 *   data DIR            the game's data directory, which the caller manages
 *   frame W H           the drawable, in pixels
 *   open 0|1            whether the game starts at the menu
 *   setting KEY VALUE   a RetroArch setting of the running game
 *   shader PATH         the preset the game is running
 *   ids ID...           ids for the case, and we report any not in the document
 *   script STEPS        ROMINABOX_MENU_SCRIPT, exactly as the launcher passes it
 *   run                 runs the case
 *
 * For each case we print one JSON line: the checkpoints from the script
 * driver, each report in the same form as in the player, and whether the
 * script ran to its end. The caller judges the result.
 *
 * Each case runs in a separate process, because some of what the menu shows
 * (status lines, slots) stays for the life of its process. For each case we
 * start the driver again with --case and write the directives of that case
 * to it.
 *
 * Here, and only here, we stand in for RetroArch. A game starts, its overlays
 * begin, and the menu opens if the game starts at the menu. A save or load
 * from the menu completes on the next frame. We write the picture of a save a
 * few frames after we report the save, in the same order as RetroArch. A
 * binding capture counts down on the clock (the fake host's timed capture). */
#include "rmlui/menu_api.h"
#include "rmlui/host.h"
#include "rmlui_bridge.h"
#include "rmlui/view.hpp"
#include "menu_host_fake.h"
#include "achievements_fake.hpp"
#include "test_arguments.h"
#include "test_environment.h"
#include "test_process.h"

#include <file/file_path.h>
#include <streams/file_stream.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace {
using rib::test::host;

/* A display refreshing at 60 Hz. */
constexpr int64_t kFrameUs = 16667;
/* Longer than any wait in a script. We report a case that has not finished
 * by then as not finished, instead of waiting for it. */
constexpr int kFrameLimit = 20000;
constexpr const char *kCheckpoint = "[RIB] checkpoint ";

struct Case
{
   std::string name, assets, data, script, shader;
   int width = 0, height = 0;
   bool open = true;
   std::vector<std::pair<std::string, float>> settings;
   std::vector<std::string> ids;
};

std::string json(const std::string& text)
{
   std::string out = "\"";
   for (unsigned char c : text)
   {
      if (c == '"' || c == '\\') { out += '\\'; out += (char)c; }
      else if (c == '\n') out += "\\n";
      else if (c < 32)
      {
         char escaped[7];
         std::snprintf(escaped, sizeof(escaped), "\\u%04x", c);
         out += escaped;
      }
      else out += (char)c;
   }
   return out + "\"";
}

/* The RetroArch state task: a save or load from the menu finishes on the
 * next frame. We report a save once its state is written (save_state_cb) and
 * before the screenshot, so the new picture appears a few frames after the
 * report, over the picture of the previous save if there was one. */
struct StateTasks
{
   /* Frames between the report of a save and its picture. */
   static constexpr int kPictureFrames = 3;
   int saves = 0, loads = 0;
   /* Frames until we write the picture of a reported save, 0 if none is due. */
   int picture_in = 0;

   void finish(const std::string& data)
   {
      if (picture_in && --picture_in == 0)
      {
         const std::string picture = data + "/states/slot-1.png";
         path_mkdir((data + "/states").c_str());
         /* Each save's picture differs from the one before it. */
         const std::string contents = "picture of save " + std::to_string(saves);
         filestream_write_file(picture.c_str(), contents.data(), (int64_t)contents.size());
         host.thumbnail = picture;
      }
      for (; saves < host.saves_started; ++saves)
      {
         host.slot_occupied = true;
         rib_rmlui_notify_state_task(host.state_path.c_str(), 1, true, true);
         picture_in = kPictureFrames;
      }
      for (; loads < host.loads_started; ++loads)
         rib_rmlui_notify_state_task(host.state_path.c_str(), 1, false, true);
   }
};

/* Redirect stderr, where we print the checkpoints of the script driver, into
 * a file for the length of one case. */
class Captured
{
public:
   Captured() : file(std::tmpfile())
   {
      std::fflush(stderr);
      saved = dup(STDERR_FILENO);
      if (file) dup2(fileno(file), STDERR_FILENO);
   }
   std::string finish()
   {
      std::fflush(stderr);
      dup2(saved, STDERR_FILENO);
      close(saved);
      std::string text;
      if (!file) return text;
      std::rewind(file);
      char buffer[4096];
      for (size_t read; (read = std::fread(buffer, 1, sizeof(buffer), file)) > 0; )
         text.append(buffer, read);
      std::fclose(file);
      return text;
   }
private:
   std::FILE *file;
   int saved = -1;
};

std::string missing_ids(const Case& run, std::vector<bool>& seen)
{
   rib::View& view = rib::menu_view();
   for (size_t index = 0; index < run.ids.size(); ++index)
      if (view.document.has_element(run.ids[index].c_str()))
         seen[index] = true;
   std::string missing = "[";
   for (size_t index = 0; index < run.ids.size(); ++index)
      if (!seen[index])
         missing += (missing.size() > 1 ? "," : "") + json(run.ids[index]);
   return missing + "]";
}

void run_case(const Case& run)
{
   host = rib::test::FakeHost{};
   host.clock_us = 1000000;
   /* The quiet window receives no mouse events, so the RetroArch pointer
    * stays where it starts. */
   host.pointer = {};
   host.save_accepted = true;
   host.load_accepted = true;
   host.timed_capture = true;
   host.menu_open = run.open;
   host.current_shader = run.shader;
   for (const auto& [key, value] : run.settings)
      host.settings[key] = value;
   session = {};
   session.status = RIB_ACHIEVEMENTS_SIGNED_OUT;
   service_rows.clear();
   saved_accounts.clear();

   test_setenv("ROMINABOX_RML_ASSETS", run.assets.c_str());
   test_setenv("ROMINABOX_DATA_DIR", run.data.c_str());
   test_setenv("ROMINABOX_MENU_SCRIPT", run.script.c_str());
   test_unsetenv("ROMINABOX_MENU_SHOT");

   Captured captured;
   StateTasks tasks;
   std::vector<bool> seen(run.ids.size(), false);
   void *menu = rib_menu_create();
   if (!menu)
   {
      captured.finish();
      std::fprintf(stderr, "%s: no menu\n", run.name.c_str());
      std::exit(1);
   }
   /* The game has started. Before we draw the first frame, its overlays
    * start, and the menu opens if the game starts at the menu. */
   rib_rmlui_begin_overlays();
   if (run.open)
      rib_menu_toggle(menu, true);
   int frames = 0;
   bool loaded = false;
   std::string missing = "[]";
   while (frames < kFrameLimit && !host.script_finished && !host.quit)
   {
      rib_menu_frame(menu, run.width, run.height);
      ++frames;
      host.clock_us += kFrameUs;
      if (!loaded && rib::menu_view().document.root())
      {
         loaded = true;
         missing = missing_ids(run, seen);
      }
      tasks.finish(run.data);
   }
   if (loaded)
      missing = missing_ids(run, seen);
   rib_menu_destroy(menu);
   const std::string printed = captured.finish();

   std::string checkpoints = "[";
   std::istringstream lines(printed);
   for (std::string line; std::getline(lines, line); )
   {
      const auto at = line.find(kCheckpoint);
      if (at == std::string::npos) continue;
      const std::string rest = line.substr(at + std::strlen(kCheckpoint));
      const auto space = rest.find(' ');
      if (space == std::string::npos) continue;
      checkpoints += (checkpoints.size() > 1 ? "," : "");
      checkpoints += "{\"label\":" + json(rest.substr(0, space))
            + ",\"report\":" + rest.substr(space + 1) + "}";
   }
   checkpoints += "]";
   std::printf("{\"case\":%s,\"loaded\":%s,\"missing\":%s,\"finished\":%s,\"quit\":%s,"
         "\"frames\":%d,\"errors\":%s,\"checkpoints\":%s}\n",
         json(run.name).c_str(), loaded ? "true" : "false", missing.c_str(),
         host.script_finished ? "true" : "false", host.quit ? "true" : "false",
         frames, json(host.error_log).c_str(), checkpoints.c_str());
   std::fflush(stdout);
}
}

int main(int argc, char **argv)
{
   /* The path of this program, which we start again for each case, as
    * UTF-8 on every platform. */
   Utf8Arguments utf8(argc, argv);
   argc = utf8.argc();
   argv = utf8.argv();
   /* With --case, this process runs one case: we read the directives of
    * that case and run it. */
   const bool one_case = argc == 2 && std::strcmp(argv[1], "--case") == 0;
   Case current;
   std::string directives;
   int cases = 0;
   for (std::string line; std::getline(std::cin, line); )
   {
      if (line.empty()) continue;
      const auto space = line.find(' ');
      const std::string word = line.substr(0, space);
      const std::string rest = space == std::string::npos ? "" : line.substr(space + 1);
      if (word == "case")
         directives.clear();
      directives += line + "\n";
      if (word == "case") current = Case{rest};
      else if (word == "assets") current.assets = rest;
      else if (word == "data") current.data = rest;
      else if (word == "script") current.script = rest;
      else if (word == "shader") current.shader = rest;
      else if (word == "open") current.open = rest == "1";
      else if (word == "frame")
      {
         std::istringstream size(rest);
         size >> current.width >> current.height;
      }
      else if (word == "setting")
      {
         const auto split = rest.find(' ');
         if (split == std::string::npos)
         {
            std::fprintf(stderr, "a setting needs a value: %s\n", line.c_str());
            return 2;
         }
         current.settings.emplace_back(rest.substr(0, split),
               std::strtof(rest.c_str() + split + 1, nullptr));
      }
      else if (word == "ids")
      {
         std::istringstream words(rest);
         for (std::string id; words >> id; ) current.ids.push_back(id);
      }
      else if (word == "run")
      {
         if (current.width <= 0 || current.height <= 0 || current.assets.empty()
               || current.data.empty())
         {
            std::fprintf(stderr, "%s: a case needs assets, data and a frame\n",
                  current.name.c_str());
            return 2;
         }
         if (one_case)
         {
            run_case(current);
            std::fflush(stdout);
            return 0;
         }
         /* Run each case in a separate process (see the top of this file). */
         std::fflush(stdout);
         const int status = run_with_input({argv[0], "--case"}, directives);
         if (status != 0)
         {
            std::fprintf(stderr, "%s: the case ended abnormally (status %d)\n",
                  current.name.c_str(), status);
            return 1;
         }
         ++cases;
      }
      else
      {
         std::fprintf(stderr, "unknown directive: %s\n", line.c_str());
         return 2;
      }
   }
   std::fprintf(stderr, "menu_workflow_driver: %d cases\n", cases);
   return cases ? 0 : 1;
}
