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
/* The service's rows, and each report of whether the list is on screen. */
#include <vector>
extern std::vector<rib_achievement_row_t> service_rows;
extern std::vector<bool> list_shown_reports;
/* QUICK SIGN IN: accounts saved in other games, and requests from the menu. */
extern std::vector<std::string> saved_accounts;
extern std::string quick_signed_in, forgotten;
