#include "achievements_fake.hpp"
#include <cstdio>

rib_achievements_snapshot_t session{};
int sign_ins;
rib_achievement_unlock_t pending_unlock{};
std::vector<rib_achievement_row_t> service_rows;
std::vector<bool> list_shown_reports;
std::vector<std::string> saved_accounts;
std::string quick_signed_in, forgotten;

static void touch() { ++session.revision; }

extern "C" {
void rib_achievements_get_snapshot(rib_achievements_snapshot_t *out) { *out = session; }
bool rib_achievements_get_row(size_t index, rib_achievement_row_t *out)
{
   if (index >= service_rows.size() || !out) return false;
   *out = service_rows[index];
   return true;
}
void rib_achievements_list_shown(bool shown)
{
   if (list_shown_reports.empty() || list_shown_reports.back() != shown) list_shown_reports.push_back(shown);
}
bool rib_achievements_has_unlocks() { return pending_unlock.id != 0; }
bool rib_achievements_take_unlock(rib_achievement_unlock_t *out)
{
   if (!pending_unlock.id) return false;
   *out = pending_unlock;
   pending_unlock = {};
   return true;
}
bool rib_achievements_has_pending_uploads() { return session.pending_upload; }
bool rib_achievements_sign_in(const char*, const char*)
{
   ++sign_ins;
   session.status = RIB_ACHIEVEMENTS_SIGNING_IN;
   touch();
   return true;
}
bool rib_achievements_set_enabled(bool on)
{
   session.status = on ? RIB_ACHIEVEMENTS_ACTIVE : RIB_ACHIEVEMENTS_OFF;
   touch();
   return true;
}
bool rib_achievements_retry() { return true; }
void rib_achievements_cancel() { session.status = RIB_ACHIEVEMENTS_SIGNED_OUT; touch(); }
void rib_achievements_sign_out()
{
   session = {};
   session.status = RIB_ACHIEVEMENTS_SIGNED_OUT;
   service_rows.clear();
   touch();
}
void rib_achievements_skip_startup() { session.startup_waiting = false; session.startup_skipped = true; touch(); }
size_t rib_achievements_saved_accounts(rib_achievements_saved_account_t *out, size_t capacity)
{
   size_t count = 0;
   for (; count < saved_accounts.size() && count < capacity; ++count)
   {
      std::snprintf(out[count].username, sizeof(out[count].username), "%s", saved_accounts[count].c_str());
      std::snprintf(out[count].display_name, sizeof(out[count].display_name), "%s", saved_accounts[count].c_str());
   }
   return count;
}
bool rib_achievements_quick_sign_in(const char *username)
{
   if (session.status != RIB_ACHIEVEMENTS_SIGNED_OUT) return false;
   quick_signed_in = username;
   session.status = RIB_ACHIEVEMENTS_SIGNING_IN;
   touch();
   return true;
}
bool rib_achievements_forget_account(const char *username)
{
   for (auto at = saved_accounts.begin(); at != saved_accounts.end(); ++at)
      if (*at == username) { saved_accounts.erase(at); forgotten = username; return true; }
   return false;
}
}
