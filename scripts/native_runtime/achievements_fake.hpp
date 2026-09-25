#pragma once
/* The achievements service in the menu tests: one stand-in for every harness.
 * We test the actual service in the native client suite (achievement-client). */
#include "rmlui/achievements.hpp"
#include <string>
#include <vector>

extern rib_achievements_snapshot_t session;
extern int sign_ins;
extern rib_achievement_unlock_t pending_unlock;
/* The service's rows, and each report of whether the list is on screen. */
extern std::vector<rib_achievement_row_t> service_rows;
extern std::vector<bool> list_shown_reports;
/* QUICK SIGN IN: accounts saved in other games, and requests from the menu. */
extern std::vector<std::string> saved_accounts;
extern std::string quick_signed_in, forgotten;
