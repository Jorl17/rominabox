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
   if (!document.initialize(argv[1], {"Silkscreen-Regular.ttf"}, 960, 600, false)) return 2;
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

   // While this popup waits behind another one, the colour badge can become
   // ready before it opens, and we do not refresh the list after that. The
   // popup must still open with the badge.
   overlays.notify({rib::Overlays::Notice::Achievement, "OTHER", "1 points", ""});
   std::snprintf(service_rows[0].badge_path, sizeof(service_rows[0].badge_path), "%s", path.c_str());
   service_rows[0].badge = RIB_ACHIEVEMENT_BADGE_READY;
   ++session.revision;
   achievements.update();
   overlays.clear_notification();
   pending_unlock = {}; pending_unlock.id = 123; pending_unlock.points = 5;
   std::snprintf(pending_unlock.title, sizeof(pending_unlock.title), "FIRST STEP");
   achievements.update(); document.settle();
   check(popup && !rib::hidden(popup) && badge && !rib::hidden(badge) &&
         badge->GetAttribute<Rml::String>("src", "") == path,
         "A queued popup whose colour badge is already on disk opens with it");

   // We show the notice from a hotkey about the save slots in the same row,
   // marked with its kind, and we do not draw on it a badge that arrives for
   // an unlock that the notice replaced.
   overlays.notify({rib::Overlays::Notice::Achievement, "SECOND STEP", "5 points", ""});
   overlays.notify({rib::Overlays::Notice::Slot, "SAVED TO SLOT 2", "", ""});
   overlays.show_badge(path);
   document.settle();
   check(popup && popup->GetAttribute<Rml::String>("data-notice", "") == "slot",
         "A slot notice marks the row as one");
   check(badge && rib::hidden(badge), "A slot notice that took an unlock's place is given no badge");
   overlays.notify({rib::Overlays::Notice::Achievement, "THIRD STEP", "5 points", ""});
   check(popup && popup->GetAttribute<Rml::String>("data-notice", "") == "achievement",
         "An unlock marks the row as one");

   // A slot notice stays in the row for less time than an unlock. An unlock
   // earned while a slot notice is shown waits, and opens when the notice
   // goes two seconds later. The unlock then stays longer than that. (The
   // designs draw a slot notice only over the game, so while the menu is
   // open we check the row for which notice it has.)
   const auto notice = [&] { return popup ? popup->GetAttribute<Rml::String>("data-notice", "") : ""; };
   const auto unlocked = [&] {
      auto *title = document.root()->GetElementById("unlock-title");
      return popup && !rib::hidden(popup) && notice() == "achievement" && title
            && title->GetInnerRML() == "FOURTH STEP";
   };
   overlays.clear_notification();
   overlays.notify({rib::Overlays::Notice::Slot, "SLOT 3", "", ""});
   pending_unlock = {}; pending_unlock.id = 124; pending_unlock.points = 5;
   std::snprintf(pending_unlock.title, sizeof(pending_unlock.title), "FOURTH STEP");
   achievements.update(); document.settle();
   check(notice() == "slot", "An unlock waits while a slot notice is up");
   host_time_us += 2000000;
   overlays.update(false);
   achievements.update(); document.settle();
   check(unlocked(), "Two seconds after a slot notice, the unlock queued behind it shows");
   host_time_us += 2000000;
   overlays.update(false); document.settle();
   check(unlocked(), "Two seconds on, the unlock still shows");

   // The row says what it does, for the design to move it. It is drawn from
   // the frame before it says it is showing, so its arrival can animate, and
   // it is leaving, still drawn, for the time the design declares before it
   // goes.
   rib::DesignDeclarations motion;
   motion.notice_row_leave_ms = 300;
   overlays.load(motion);
   overlays.clear_notification();
   // The frame it comes up in, then the next one.
   overlays.notify({rib::Overlays::Notice::Achievement, "FIFTH STEP", "5 points", ""});
   overlays.update(false); document.settle();
   check(popup && !rib::hidden(popup) && !popup->IsClassSet("showing"),
         "A popup is drawn for a frame before it says it is showing");
   overlays.update(false); document.settle();
   check(popup && popup->IsClassSet("showing"), "On the next frame the popup is showing");
   host_time_us += 4500 * 1000;
   overlays.update(false); document.settle();
   check(popup && !rib::hidden(popup) && popup->IsClassSet("leaving") && !popup->IsClassSet("showing"),
         "When its time is up, the popup is leaving and still drawn");
   check(overlays.drawing(), "We draw over the game while the popup leaves");
   host_time_us += 300 * 1000;
   overlays.update(false); document.settle();
   check(popup && rib::hidden(popup) && !popup->IsClassSet("leaving"), "Once it has left, the popup is hidden");

   achievements.context_lost();
   document.shutdown();
   std::printf("unlock popup: %d failures\n", failures);
   return failures ? 1 : 0;
}
