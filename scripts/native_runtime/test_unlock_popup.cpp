/* The unlock popup of an achievement earned for the first time, whose
 * colour badge is not on disk yet. With the production menu presenter and
 * RmlUi document, the achievements service from account_test_host, and no window. */
#include "rmlui/document.hpp"
#include "rmlui/elements.hpp"
#include "rmlui/achievements.hpp"
#include "account_test_host.hpp"
#include <cstdio>
#include <string>

int main(int argc, char **argv) {
   if (argc < 2) return 2;
   int failures = 0;
   auto check = [&](bool passed, const char *message) {
      if (!passed) { std::fprintf(stderr, "FAIL: %s\n", message); ++failures; }
   };
   rib::Document document;
   if (!document.initialize(argv[1], 960, 600, false)) return 2;
   document.show(); document.settle();
   rib::EventQueue events;
   rib::Lists lists(document, events);
   rib::Overlays overlays(document);
   rib::Achievements achievements(document, lists, events, overlays);
   achievements.bind();

   rib_achievement_row_t row{};
   row.id = 123; row.points = 5; row.state = RIB_ACHIEVEMENT_UNLOCKED;
   row.badge = RIB_ACHIEVEMENT_BADGE_LOADING;
   std::snprintf(row.title, sizeof(row.title), "FIRST STEP");
   service_rows = {row};
   session = {}; session.status = RIB_ACHIEVEMENTS_ACTIVE; session.count = 1; session.revision = 1;
   std::snprintf(session.account, sizeof(session.account), "fixture");
   pending_unlock = {}; pending_unlock.id = 123; pending_unlock.points = 5;
   std::snprintf(pending_unlock.title, sizeof(pending_unlock.title), "FIRST STEP");
   achievements.update(); document.settle();

   auto *popup = document.root()->GetElementById("unlock-row");
   auto *badge = document.root()->GetElementById("unlock-badge");
   check(popup && !rib::hidden(popup), "The unlock popup is up");
   check(badge && rib::hidden(badge), "No badge is drawn before the picture exists");

   const std::string path = document.asset_path("badge-123.png");
   service_rows[0].badge = RIB_ACHIEVEMENT_BADGE_READY;
   std::snprintf(service_rows[0].badge_path, sizeof(service_rows[0].badge_path), "%s", path.c_str());
   ++session.revision;
   achievements.update(); document.settle();
   check(badge && !rib::hidden(badge) && badge->GetAttribute<Rml::String>("src", "") == path,
         "the popup shows the colour badge once it arrives");

   // A badge that arrives after the popup has gone brings nothing back.
   overlays.clear_notification();
   service_rows[0].badge_path[0] = '\0';
   service_rows[0].badge = RIB_ACHIEVEMENT_BADGE_LOADING;
   pending_unlock = {}; pending_unlock.id = 123; pending_unlock.points = 5;
   ++session.revision;
   achievements.update();
   overlays.clear_notification();
   std::snprintf(service_rows[0].badge_path, sizeof(service_rows[0].badge_path), "%s", path.c_str());
   service_rows[0].badge = RIB_ACHIEVEMENT_BADGE_READY;
   ++session.revision;
   achievements.update(); document.settle();
   check(popup && rib::hidden(popup), "A badge arriving after the popup has gone does not reopen it");

   achievements.context_lost();
   document.shutdown();
   std::printf("unlock popup: %d failures\n", failures);
   return failures ? 1 : 0;
}
