#include "rmlui/document.hpp"
#include "rmlui/text_entry.hpp"
#include "rmlui/text_host.h"
#include "rmlui/screens.hpp"
#include "rmlui/elements.hpp"
#include "rmlui/achievements.hpp"
#include "rmlui/saved_accounts.hpp"
#include <cstring>
#include <RmlUi/Core/Elements/ElementFormControlInput.h>
#include <libretro.h>
#include <cstdio>
#include <string>
#include <fstream>
#include <cmath>
#include "account_test_host.hpp"

struct Clicks : Rml::EventListener {
   int count = 0;
   void ProcessEvent(Rml::Event&) override { ++count; }
};
int main(int argc, char **argv) {
   if (argc < 2) return 2;
   int failures = 0;
   auto check = [&](bool passed, const char *message) {
      if (!passed) { std::fprintf(stderr, "FAIL: %s\n", message); ++failures; }
   };
   rib::Document document;
   if (!document.initialize(argv[1], 960, 600, false)) return 2;
   document.set_shown("pause-panel", false);
   document.set_shown("achievements-panel", true);
   document.set_shown("achievements-signed-out", false);
   document.set_shown("achievements-form", true);
   document.show(); document.settle();
   rib::TextEntry entry(document);
   entry.bind(); entry.enable("achievements-form", "achievements-submit", "achievements-cancel");
   int x, y, width, height;
   document.element_box("achievements-submit", &x, &y, &width, &height);
   check(width >= 200 && height >= 36, "Sign in is a full sized button");
   document.element_box("achievements-cancel", &x, &y, &width, &height);
   check(width >= 200 && height >= 36, "Cancel is a full sized button");
   int py, ph, unused, ry, rh;
   document.element_box("achievement-password", &unused, &py, &unused, &ph);
   document.element_box("achievements-password-visibility", &unused, &ry, &unused, &rh);
   check(std::abs((2 * ry + rh) - (2 * py + ph)) <= 2,
         "Show button is vertically centered in the password field");
   check(ry - py >= 4 && py + ph - ry - rh >= 4,
         "Show button has inner padding above and below");
   auto *username = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-username"));
   auto *password = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-password"));
   check(username && password, "Fields are real RmlUi text controls");
   if (!username || !password) return 1;
   username->Focus();
   entry.physical(true, RETROK_a, 'a', 0);
   entry.physical(false, RETROK_a, 0, 0);
   check(username->GetValue() == "a", "Physical keyboard types into username");
   entry.physical(true, RETROK_DOWN, 0, 0);
   check(document.get_context()->GetFocusElement() == password, "Keyboard Down uses the same form navigation as the joypad");
   entry.controller(RIB_KEY_UP);
   check(document.get_context()->GetFocusElement() == username, "Joypad Up returns to the previous field");
   {
      /* Tab moves through the fields and buttons of the form and wraps.
       * Shift+Tab moves back. */
      const char *order[] = {"achievement-password", "achievements-password-visibility",
            "achievements-submit", "achievements-cancel", "achievement-username"};
      bool forward = true;
      for (const char *id : order) {
         entry.physical(true, RETROK_TAB, '\t', 0);
         auto *at = document.get_context()->GetFocusElement();
         forward = forward && at && at->GetId() == id;
      }
      check(forward, "Tab cycles username, password, SHOW, SIGN IN, CANCEL and back to username");
      entry.physical(true, RETROK_TAB, '\t', RETROKMOD_SHIFT);
      auto *at = document.get_context()->GetFocusElement();
      check(at && at->GetId() == "achievements-cancel", "Shift+Tab goes back from the username to CANCEL");
      username->Focus();
   }
   password->Focus();
   entry.physical(true, RETROK_p, 'p', 0);
   entry.physical(true, RETROK_UNKNOWN, 0xe9, 0);
   check(password->GetValue() == "p\xc3\xa9", "Text entry retains Unicode password characters");
   check(password->GetAttribute<std::string>("type", "") == "password", "Password uses masked control");
   entry.physical(true, RETROK_a, 0x105, RETROKMOD_CTRL | RETROKMOD_ALT);
   check(password->GetValue() == "p\xc3\xa9\xc4\x85", "Committed AltGr text is preserved without invoking Ctrl-A");
   Rml::GetSystemInterface()->SetClipboardText("-paste");
   entry.physical(true, RETROK_v, 0, RETROKMOD_META);
   check(password->GetValue() == "p\xc3\xa9\xc4\x85-paste", "Command-V pastes through the clipboard boundary");
   username->Focus();
   entry.controller(RIB_KEY_OK);
   check(keyboard_active && entry.keyboard_open(), "Controller opens RetroArch keyboard");
   rib_host_keyboard_choose(0); entry.update();
   check(username->GetValue() == "ax", "Controller keyboard updates the field");
   entry.cancel_keyboard();
   check(username->GetValue() == "a" && !keyboard_active, "Cancel restores original field and closes keyboard");
   Clicks cancel, submit;
   auto *cancel_button = document.root()->GetElementById("achievements-cancel");
   auto *submit_button = document.root()->GetElementById("achievements-submit");
   cancel_button->AddEventListener(Rml::EventId::Click, &cancel);
   submit_button->AddEventListener(Rml::EventId::Click, &submit);
   cancel_button->Focus();
   entry.physical(true, RETROK_RETURN, '\r', 0);
   check(cancel.count == 1 && submit.count == 0, "Enter activates the focused Cancel button");
   // Alt+Enter is the fullscreen chord, never the form's Enter.
   check(!entry.physical(true, RETROK_RETURN, '\r', RETROKMOD_ALT) && cancel.count == 1 && submit.count == 0,
         "the sign-in form leaves Alt+Return to the fullscreen chord");
   check(!entry.physical(false, RETROK_RETURN, 0, RETROKMOD_ALT),
         "the sign-in form leaves the chord's release alone too");
   submit_button->Focus();
   check(!entry.physical(true, RETROK_KP_ENTER, '\r', RETROKMOD_ALT) && submit.count == 0,
         "the keypad's Alt+Enter is not the form's either");
   entry.disable();
   check(!entry.physical(true, RETROK_a, 'a', 0), "Closed form releases keyboard routing");
   rib::EventQueue events;
   rib::Event hovered;
   rib::Screens screens(document, events, hovered);
   screens.declare_screen("achievements", "achievements-panel", "ACHIEVEMENTS", "BACK", "achievements");
   screens.declare_screen("achievements", "achievements-panel", "ACHIEVEMENTS", "BACK", "achievements");
   document.root()->GetElementById("achievements")->Click();
   check(events.take().kind == RIB_RMLUI_ACTION_SHOW_SCREEN, "Achievements entry opens before context recreation");
   check(events.take().kind == RIB_RMLUI_ACTION_NONE, "Repeated declarations attach only one listener");
   document.shutdown();
   check(document.initialize(argv[1], 960, 600, false), "Replacement document loads");
   screens.clear_screens();
   screens.declare_screen("achievements", "achievements-panel", "ACHIEVEMENTS", "BACK", "achievements");
   document.root()->GetElementById("achievements")->Click();
   check(events.take().kind == RIB_RMLUI_ACTION_SHOW_SCREEN, "Achievements entry opens after context recreation");
   document.shutdown();
   check(document.initialize(argv[1], 960, 600, false), "Presenter document loads");
   document.set_shown("pause-panel", false);
   document.set_shown("achievements-panel", true);
   document.show(); document.settle();
   screens.clear_screens();
   screens.declare_screen("pause", "pause-panel", "PAUSED", "ESC CONTINUE", "");
   screens.declare_screen("achievements", "achievements-panel", "ACHIEVEMENTS", "ESC BACK", "achievements");
   screens.show_screen("achievements");
   auto capture = [&](const char *state) {
      if (argc < 3) return;
      document.settle();
      std::ofstream out(document.asset_path((std::string("account-") + state + ".rml").c_str()));
      out << "<rml><head><link type=\"text/rcss\" href=\"menu.rcss\"/></head><body>"
          << document.root()->GetInnerRML() << "</body></rml>";
   };
   rib::Lists lists(document, events);
   rib::Overlays overlays(document);
   rib::Achievements achievements(document, lists, events, overlays);
   session.status = RIB_ACHIEVEMENTS_SIGNED_OUT; session.revision = 1;
   achievements.bind();
   capture("signed-out");
   document.set_shown("achievements-signed-out", false);
   document.set_shown("achievements-catalog", true);
   std::vector<rib::Lists::Row> rows = {{"test-row", "FIRST ACHIEVEMENT", "Earn this in the fixture", "5 PT / LOCKED", "", false}};
   lists.replace_rows("achievements-list", rows);
   auto *row = document.root()->GetElementById("test-row");
   row->SetClass("focused", true);
   Clicks row_click;
   row->AddEventListener(Rml::EventId::Click, &row_click);
   rows[0].state = "5 PT / EARNED";
   lists.replace_rows("achievements-list", rows);
   row = document.root()->GetElementById("test-row");
   row->Click(); events.clear();
   check(row->IsClassSet("focused") && row_click.count == 1,
         "A live row state update retains its focus and attached interaction");
   row->RemoveEventListener(Rml::EventId::Click, &row_click);
   document.click_element("achievements-login");
   achievements.handle(events.take()); document.settle();
   achievements.update();
   capture("sign-in");
   auto *focus = document.get_context()->GetFocusElement();
   check(focus && focus->GetId() == "achievement-username", "Opening sign in focuses the visible username field");
   password = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-password"));
   password->SetValue("synthetic-password");
   password->Focus();
   password->SetSelectionRange(3, 3);
   // The text of the field is in its #text children. After Show and Hide
   // they must still be inside the field, not at the corner of the window.
   auto text_inside_field = [&](const char *when) {
      document.settle();
      auto *field = document.root()->GetElementById("achievement-password");
      const Rml::Vector2f at = field->GetAbsoluteOffset(Rml::BoxArea::Border);
      const Rml::Vector2f size = field->GetBox().GetSize(Rml::BoxArea::Border);
      int texts = 0;
      bool inside = true;
      for (int index = 0; index < field->GetNumChildren(true); ++index) {
         auto *child = field->GetChild(index);
         if (child->GetTagName() != "#text") continue;
         ++texts;
         const Rml::Vector2f offset = child->GetAbsoluteOffset(Rml::BoxArea::Border);
         inside = inside && offset.x >= at.x && offset.y >= at.y
               && offset.x <= at.x + size.x && offset.y <= at.y + size.y;
      }
      char message[160];
      std::snprintf(message, sizeof(message), "the password's text lies inside its field after %s", when);
      check(texts > 0 && inside, message);
   };
   text_inside_field("opening the form");
   achievements.handle(rib::Event::account_action(rib::AccountAction::RevealPassword));
   password = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-password"));
   check(password->GetAttribute<std::string>("type", "") == "text" && password->GetValue() == "synthetic-password", "Show password reveals the existing value");
   text_inside_field("Show");
   {
      int start = -1, end = -1;
      password->GetSelection(&start, &end, nullptr);
      check(document.get_context()->GetFocusElement() == password && start == 3 && end == 3,
            "Show keeps the field focused with its caret where it was");
   }
   achievements.handle(rib::Event::account_action(rib::AccountAction::RevealPassword));
   password = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-password"));
   check(password->GetAttribute<std::string>("type", "") == "password" && password->GetValue() == "synthetic-password",
         "Hide masks the existing value again");
   text_inside_field("Hide");
   check(document.get_context()->GetFocusElement() == password, "Hide keeps the field focused");
   achievements.handle(rib::Event::account_action(rib::AccountAction::RevealPassword));
   achievements.leave_form();
   password = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-password"));
   check(password->GetAttribute<std::string>("type", "") == "password" && password->GetValue().empty(), "Closing the form clears and masks the password");
   achievements.handle(rib::Event::account_action(rib::AccountAction::Open));
   session.pending_upload = true;
   check(achievements.request_exit(rib::Achievements::Exit::Quit), "Pending upload requires confirmation");
   check(achievements.handle({RIB_RMLUI_ACTION_RESUME}), "Confirmation consumes underlying Resume action");
   check(achievements.physical(true, RETROK_a, 'a', 0), "Confirmation consumes text without editing the form beneath it");
   username = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-username"));
   check(username->GetValue().empty(), "Modal text does not reach the underlying username");
   capture("confirmation");
   check(!achievements.physical(true, RETROK_RETURN, '\r', RETROKMOD_ALT) && achievements.modal() && quits == 0,
         "the confirmation leaves Alt+Return to the fullscreen chord");
   achievements.physical(true, RETROK_ESCAPE, 0, 0);
   check(!achievements.modal(), "Physical Escape cancels the pending-upload confirmation");
   check(document.get_context()->GetFocusElement() == username, "Escape restores the form's focused field");
   achievements.physical(true, RETROK_UNKNOWN, 0xe9, 0);
   check(username->GetValue() == "\xc3\xa9", "Typing after confirmation resumes in the same field");
   password->Focus();
   achievements.request_exit(rib::Achievements::Exit::Quit);
   achievements.handle(rib::Event::account_action(rib::AccountAction::KeepSession));
   check(!achievements.modal() && quits == 0, "Keeping session cancels quit");
   check(document.get_context()->GetFocusElement() == password, "Keep Playing restores password focus");
   achievements.key(RIB_KEY_UP);
   check(document.get_context()->GetFocusElement() == username, "Joypad continues from the restored form control");
   achievements.leave_form();
   session.startup_waiting = true; session.status = RIB_ACHIEVEMENTS_LOADING; ++session.revision;
   achievements.update();
   capture("startup");
   achievements.physical(true, RETROK_RETURN, '\r', 0);
   achievements.handle(events.take());
   check(session.startup_skipped, "Physical Enter activates Play without achievements");
   session.startup_waiting = false; ++session.revision; achievements.update();
   pending_unlock.id = 123; pending_unlock.points = 5; pending_unlock.pending_upload = true;
   std::snprintf(pending_unlock.title, sizeof(pending_unlock.title), "EARNED");
   achievements.update();
   session.pending_upload = false; ++session.revision; achievements.update();
   check(document.root()->GetElementById("unlock-detail")->GetInnerRML() == "5 points",
         "Unlock toast does not retain a stale upload status after acknowledgement");
   overlays.clear_notification();
   overlays.notify({"A LONG ACHIEVEMENT TITLE", "5 points", ""});
   capture("notification");
   rib_design_data design{};
   document.shutdown();
   check(document.initialize(argv[1], 960, 600, false), "Notification document reloads");
   overlays.load(design);
   document.settle();
   auto *notice = document.root()->GetElementById("unlock-row");
   check(notice && !rib::hidden(notice),
         "The active unlock notification survives a document rebuild");
   overlays.stop();
   check(!overlays.drawing(), "Stopping overlays clears the active notification");
   overlays.update(false);
   check(!overlay_frames, "Stopped overlays release closed-menu rendering on the next frame");
   overlays.notify({"CLEAR", "5 points", ""});
   overlays.clear_notification();
   overlays.update(false);
   check(!overlay_frames, "Cleared notification releases closed-menu rendering");
   overlays.notify({"EXPIRES", "5 points", ""});
   host_time_us += 5000000;
   overlays.update(true);
   check(overlays.drawing() && overlay_frames, "A pending script keeps the render callback active after notification expiry");
   overlays.update(false);
   check(!overlays.drawing() && !overlay_frames, "Finishing that script releases rendering");
   overlays.begin();
   overlays.update(true);
   check(overlays.drawing(), "A capture continues after the startup overlays finish");
   overlays.update(false);
   check(!overlays.drawing(), "Finished startup capture releases rendering");
   {
      // While a badge downloads we show a moving placeholder. When the
      // download finishes while the list is closed, only the picture
      // changes.
      document.shutdown();
      check(document.initialize(argv[1], 960, 600, false), "Badge document loads");
      document.show(); document.settle();
      achievements.context_lost();
      achievements.bind();
      auto row_of = [](uint32_t id, rib_achievement_badge_t badge, const char *path) {
         rib_achievement_row_t row{};
         row.id = id; row.points = 5; row.state = RIB_ACHIEVEMENT_LOCKED; row.badge = badge;
         std::snprintf(row.title, sizeof(row.title), "BADGE %u", id);
         std::snprintf(row.badge_path, sizeof(row.badge_path), "%s", path);
         return row;
      };
      const std::string ready_path = document.asset_path("badge-1.png");
      service_rows = {row_of(1, RIB_ACHIEVEMENT_BADGE_READY, ready_path.c_str()),
                      row_of(2, RIB_ACHIEVEMENT_BADGE_LOADING, "")};
      session = {}; session.status = RIB_ACHIEVEMENTS_ACTIVE; session.count = service_rows.size();
      std::snprintf(session.account, sizeof(session.account), "fixture"); session.revision = 50;
      document.set_shown("pause-panel", false);
      document.set_shown("achievements-panel", true);
      achievements.update();
      list_shown_reports.clear();
      achievements.update(); document.settle();
      // The first element of a class in a row, when it is drawn.
      auto shown_part = [&](const char *row, const char *part) -> Rml::Element * {
         auto *element = document.root()->GetElementById(row);
         std::vector<Rml::Element*> found;
         if (element) rib::collect(element, part, found);
         return !found.empty() && !rib::hidden(found[0])
               && found[0]->GetBox().GetSize(Rml::BoxArea::Border).x > 0 ? found[0] : nullptr;
      };
      auto icon_shown = [&](const char *row) { return shown_part(row, "list-row-icon") != nullptr; };
      // The placeholder moves when something in it is animated.
      auto animated = [](Rml::Element *placeholder) {
         bool moving = false;
         rib::walk(placeholder, [&](Rml::Element *element) {
            const Rml::Property *property = element->GetProperty("animation");
            moving = moving || (property && property->unit == Rml::Unit::ANIMATION
                  && !property->value.GetReference<Rml::AnimationList>().empty());
            return rib::Walk::Continue;
         });
         return moving;
      };
      auto *waiting = shown_part("achievement-2", "list-row-wait");
      check(document.root()->GetElementById("achievement-2")
            && document.root()->GetElementById("achievement-2")->IsClassSet("badge-loading")
            && waiting && !icon_shown("achievement-2"),
            "a badge still downloading shows its placeholder, not an empty image");
      check(waiting && animated(waiting), "the design animates the placeholder");
      // A declared animation does not prove that anything moves. Run the
      // list as in the menu, a frame at a time with the clock running, while
      // other badges arrive, and watch which cells are lit.
      {
         auto lit = [&]() {
            std::string pattern;
            std::vector<Rml::Element*> cells;
            if (waiting) rib::collect(waiting, "list-row-wait-cells", cells);
            if (!cells.empty())
               for (int index = 0; index < cells[0]->GetNumChildren(); ++index)
                  pattern += cells[0]->GetChild(index)->GetProperty("background-color")->ToString() + "|";
            return pattern;
         };
         std::vector<std::string> seen;
         for (int frame = 0; frame < 40; ++frame) {
            document.advance(1.0 / 60.0);
            if (frame % 10 == 9) {
               service_rows.push_back(row_of(100 + frame, RIB_ACHIEVEMENT_BADGE_READY, ready_path.c_str()));
               session.count = service_rows.size();
               ++session.revision;
            }
            achievements.update();
            document.settle();
            const std::string now = lit();
            if (seen.empty() || seen.back() != now) seen.push_back(now);
         }
         check(seen.size() >= 3, "the waiting placeholder steps round while the menu runs and badges arrive");
      }
      check(!shown_part("achievement-1", "list-row-wait"), "a downloaded badge has no placeholder");
      check(document.root()->GetElementById("achievement-1")
            && !document.root()->GetElementById("achievement-1")->IsClassSet("badge-loading") && icon_shown("achievement-1"),
            "a downloaded badge shows its picture");
      document.set_shown("achievements-panel", false);
      achievements.update();
      service_rows[1] = row_of(2, RIB_ACHIEVEMENT_BADGE_READY, ready_path.c_str());
      ++session.revision;
      achievements.update();
      document.set_shown("achievements-panel", true);
      achievements.update(); document.settle();
      auto *second = document.root()->GetElementById("achievement-2");
      check(second && !second->IsClassSet("badge-loading") && icon_shown("achievement-2")
            && !shown_part("achievement-2", "list-row-wait"),
            "a badge that arrived while the list was closed is shown when it opens");
      check(list_shown_reports == std::vector<bool>({true, false, true}),
            "the menu reports when the list opens again, so a failed badge is retried");
   }
   {
      /* QUICK SIGN IN: we show the button while accounts are saved and list a
       * row for each. Choosing a row signs in with it, and FORGET removes it. */
      auto *root = document.root();
      auto shown = [&](const char *id) {
         auto *element = root->GetElementById(id);
         return element && !rib::hidden(element);
      };
      auto text_of = [&](const char *id) {
         auto *element = root->GetElementById(id);
         return element ? std::string(element->GetInnerRML()) : std::string("(missing)");
      };
      achievements.leave_form();
      session = {}; session.status = RIB_ACHIEVEMENTS_SIGNED_OUT; ++session.revision;
      saved_accounts.clear();
      achievements.update(); achievements.shown(); document.settle();
      check(root->GetElementById("achievements-quick") && !shown("achievements-quick"),
            "QUICK SIGN IN is not offered while no account is saved");
      saved_accounts = {"JOAO", "KID"};
      achievements.shown(); document.settle();
      check(shown("achievements-quick"), "QUICK SIGN IN is offered once another game saved an account");

      rib_screen_declaration declared{};
      std::strcpy(declared.id, "accounts");
      std::strcpy(declared.role, "accounts");
      rib_design_data design{};
      design.screens = &declared;
      design.screen_count = 1;
      rib::SavedAccounts accounts(document, lists, events);
      accounts.configure(design);
      accounts.bind();
      document.set_shown("achievements-panel", false);
      document.set_shown("accounts-panel", true);
      accounts.shown(); document.settle();
      check(text_of("account-0-title") == "JOAO" && text_of("account-1-title") == "KID",
            "the accounts list has a row for each saved account, newest first");
      check(accounts.choose("account-1") && quick_signed_in == "KID",
            "choosing a row signs in with that account");
      check(accounts.leave_for() == rib::ScreenRole::Achievements,
            "after choosing, the player goes back to Achievements");
      session.status = RIB_ACHIEVEMENTS_SIGNED_OUT;

      while (events.take().kind != RIB_RMLUI_ACTION_NONE) {}
      root->GetElementById("accounts-forget")->Click();
      const rib::Event pressed = events.take();
      check(pressed.kind == RIB_RMLUI_ACTION_LIST_ACTION && pressed.id == "accounts-forget",
            "FORGET is the accounts screen's own button");
      check(accounts.act("accounts-forget") && root->GetElementById("accounts-forget")->IsClassSet("on")
            && root->GetElementById("accounts-panel")->IsClassSet("on"),
            "FORGET on is published as the on state, for the design to show");
      quick_signed_in.clear();
      check(accounts.choose("account-0") && forgotten == "JOAO" && quick_signed_in.empty(),
            "with FORGET on, a row removes that account and signs nothing in");
      document.settle();
      check(text_of("account-0-title") == "KID" && !root->GetElementById("account-1"),
            "the forgotten account leaves the list at once");
      accounts.shown();
      check(!root->GetElementById("accounts-forget")->IsClassSet("on"), "FORGET is off again when the screen is shown");
      document.set_shown("accounts-panel", false);
      document.set_shown("achievements-panel", true);
   }
   achievements.context_lost();
   document.shutdown();
   std::printf("account input: %d failures\n", failures);
   return failures ? 1 : 0;
}
