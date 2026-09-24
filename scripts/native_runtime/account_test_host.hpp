#pragma once
#include "rmlui/achievements.hpp"
#include <string>
extern std::string keyboard_value;
extern bool keyboard_active;
extern rib_achievements_snapshot_t session;
extern int sign_ins, quits;
extern bool overlay_frames;
extern int64_t host_time_us;

extern rib_achievement_unlock_t pending_unlock;
