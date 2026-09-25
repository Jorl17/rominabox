/* A fake RetroArch host for headless menu programs. From the menu code we
 * call the rib_host_* functions, and here we answer from fields set in a
 * test and record the commands. The keyboard (text_test_host.cpp) and the
 * achievements service are separate boundaries with their own fakes. */
#pragma once
#include "rmlui/host.h"
#include <string>
#include <vector>

namespace rib::test {
enum class Sound { ScrollUp, ScrollDown, Ok, Cancel };

struct FakeHost
{
   rib_pointer pointer{};
   int64_t clock_us = 0;

   /* Bindings and capture. */
   std::vector<std::string> bind_ids, loaded_ids;
   std::string captured_id;
   bool capture_start_accepted = true;
   rib_capture_result capture_result = RIB_CAPTURE_PENDING;
   float capture_remaining = 9.0f;
   bool capture_accepts_pointer = false;
   int captures_started = 0;
   int captures_cancelled = 0;

   /* Save states. Only slot 1 has a path. */
   std::string state_path = "/headless-game/slot-1.state";
   int selected_slot = 0;
   bool slot_occupied = false;
   bool save_accepted = false;
   bool load_accepted = false;
   int saves_started = 0;
   int loads_started = 0;

   /* Discs, labelled "Disc 1".."Disc N". The chosen one becomes current. */
   unsigned disc_count = 0;
   unsigned disc_index = 0;
   /* The last shader applied from the menu, and its preset. */
   std::string applied_shader, applied_preset;

   float volume_db = -12.0f;

   /* The commands from the menu to the host. */
   std::vector<Sound> sounds;
   bool quit = false;
   bool script_finished = false;
   std::string error_log;
};

extern FakeHost host;
}
