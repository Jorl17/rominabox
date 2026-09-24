/* The external service and RetroArch host boundary for the RmlUi account tests. */
#include "account_test_host.hpp"
#include "rmlui/text_host.h"
// In the focused presenter checks we replace the service boundary. In the
// separate native client/evaluator suite we test the achievements backend.
rib_achievements_snapshot_t session{};
rib_achievement_unlock_t pending_unlock{};
int sign_ins, quits;
bool overlay_frames;
int64_t host_time_us = 1000000;
std::vector<rib_achievement_row_t> service_rows;
std::vector<bool> list_shown_reports;
extern "C" {
void rib_achievements_get_snapshot(rib_achievements_snapshot_t *out) { *out = session; }
bool rib_achievements_get_row(size_t index, rib_achievement_row_t *out) {
   if (index >= service_rows.size() || !out) return false;
   *out = service_rows[index]; return true;
}
void rib_achievements_list_shown(bool shown) {
   if (list_shown_reports.empty() || list_shown_reports.back() != shown) list_shown_reports.push_back(shown);
}
bool rib_achievements_has_unlocks() { return false; }
bool rib_achievements_take_unlock(rib_achievement_unlock_t *out) {
   if (!pending_unlock.id) return false;
   *out = pending_unlock; pending_unlock = {}; return true;
}
bool rib_achievements_has_pending_uploads() { return session.pending_upload; }
bool rib_achievements_sign_in(const char*, const char*) {
   ++sign_ins; session.status = RIB_ACHIEVEMENTS_SIGNING_IN; ++session.revision; return true;
}
bool rib_achievements_set_enabled(bool on) { session.status = on ? RIB_ACHIEVEMENTS_ACTIVE : RIB_ACHIEVEMENTS_OFF; ++session.revision; return true; }
bool rib_achievements_retry() { return true; }
void rib_achievements_cancel() { session.status = RIB_ACHIEVEMENTS_SIGNED_OUT; ++session.revision; }
void rib_achievements_sign_out() { session = {}; session.status = RIB_ACHIEVEMENTS_SIGNED_OUT; ++session.revision; }
void rib_achievements_skip_startup() { session.startup_skipped = true; ++session.revision; }
int64_t rib_host_time_us() { return host_time_us; }
void rib_host_overlay_frames(bool on) { overlay_frames = on; }
void rib_host_quit() { ++quits; }
}
