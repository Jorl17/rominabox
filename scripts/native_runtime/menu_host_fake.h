/* A fake RetroArch host for headless menu programs. From the menu code we
 * call the rib_host_* functions, and here we answer from fields set in a
 * test and record the commands. The keyboard (text_test_host.cpp) and the
 * achievements service are separate boundaries with their own fakes. */
#pragma once
#include "rmlui/host.h"
#include "rmlui/menu_api.h"
#include <map>
#include <string>
#include <vector>

namespace rib::test {
enum class Sound { ScrollUp, ScrollDown, Ok, Cancel, LevelUp, LevelDown };

struct FakeHost
{
   rib_pointer pointer{};
   int64_t clock_us = 0;

   /* Bindings and capture. */
   std::vector<std::string> bind_ids, loaded_ids;
   /* The key bound to every control, by its name in a RetroArch config
    * (input_key_names.inc). */
   std::string bound_key = "a";
   /* The control whose new binding clashes with every other, or none. */
   std::string clashing;
   std::string captured_id;
   bool capture_start_accepted = true;
   /* The answer to a poll. Each new capture starts as pending. */
   rib_capture_result capture_result = RIB_CAPTURE_PENDING;
   float capture_remaining = 9.0f;
   bool capture_accepts_pointer = false;
   int captures_started = 0;
   int captures_cancelled = 0;

   /* The hotkeys. The keys and pad inputs pressed now, by a key's name in a
    * RetroArch config and a pad input's id, and the result of a capture for
    * each, in the binding format of hotkeys.inc. Any name is a key, and the
    * pad inputs are the positions on the standard pad and home. */
   std::vector<std::string> keys_down, pads_down;
   std::string captured_input;
   int input_captures_started = 0;

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
   /* Set during the menu's frame in the video driver. Applying a shader then
    * switches to the GL context of a hardware core in the middle of that
    * frame, and the window has the game without the menu. */
   bool drawing = false;
   int applied_while_drawing = 0;
   /* The last controller passed to the core: the pad's id and its libretro
    * device. */
   std::string applied_device;
   unsigned applied_libretro = 0;

   /* The RetroArch settings behind each player setting, by config key (its
    * name in settings.inc), with their values in the running game. We cannot
    * apply a key that is missing here. */
   std::map<std::string, float> settings{{"audio_volume", -12.0f}, {"pause_nonactive", 1.0f},
         {"input_rumble_enable", 1.0f}};
   /* Whether the core requested the rumble interface. For the generated Mega
    * Drive cartridge it is not requested, so rumble has no effect there. */
   bool rumbles = false;
   /* The game volume at each request for a level cue, in order. */
   std::vector<float> level_cue_db;
   /* The level cue file requested for a game with no sound pack. */
   std::string level_cue;

   /* The commands from the menu to the host. */
   std::vector<Sound> sounds;
   bool quit = false;
   bool forgotten = false;
   bool script_finished = false;
   std::string error_log;

   /* Whether RetroArch's menu is open. When a game does not start at the
    * menu, the menu is closed and we draw only its overlays. */
   bool menu_open = true;
   /* The thumbnail beside an occupied slot 1 in RetroArch, or empty. */
   std::string thumbnail;
   /* The aspect ratio of the running game, width over height, as reported
    * by RetroArch for the core. It can change while the game runs. */
   float game_aspect = 4.0f / 3.0f;
   /* The preset in use in the running game. Applying a preset makes it the
    * one in use, as in RetroArch. */
   std::string current_shader;
   /* The time requested from the menu for the last capture. */
   unsigned capture_seconds = 0;
   /* A capture with a countdown on the clock from the given seconds, then a
    * timeout, as in RetroArch. When this is off, capture_result and
    * capture_remaining keep the values set in a test. */
   bool timed_capture = false;
   int64_t capture_began_us = 0;
};

extern FakeHost host;

/* One pass of the RetroArch loop with the menu: we carry out the player's
 * requests between frames, then draw the menu's frame in the video driver. */
inline void loop_pass(void *menu, int width, int height)
{
   rib_menu_update(menu);
   host.drawing = true;
   rib_menu_frame(menu, width, height);
   host.drawing = false;
}
}
