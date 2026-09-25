/* The RmlUi editor and the Cocoa text-client protocol, with no window or
 * keymap simulation. Dead keys and IME candidate placement must be tested by
 * hand. We include the adapter, so its OS client can stay private. */
#include "../../vendor/retroarch/menu/drivers/rmlui/text_input_macos.mm"
#include <cstdio>
#include "account_test_host.hpp"
extern "C" bool rib_rmlui_begin_native_text() { return true; }
int main(int argc, char **argv)
{
   if (argc < 2) return 2;
   @autoreleasepool {
      int failures = 0;
      auto check = [&](bool pass, const char *message) {
         if (!pass) { std::fprintf(stderr, "FAIL composition: %s\n", message); ++failures; }
      };
      check(character_offset(@"😀é", 2) == 1 && utf16_offset(@"😀é", 1) == 2,
            "UTF-16 and Unicode codepoint ranges agree across surrogate pairs");
      check(character_offset(@"😀é", 1) == 0, "A replacement cannot split a surrogate pair");
      rib::Document document;
      if (!document.initialize(argv[1], {"Silkscreen-Regular.ttf"}, 960, 600, false)) return 2;
      document.set_shown("pause-panel", false);
      document.set_shown("achievements-panel", true);
      document.set_shown("achievements-form", true);
      document.show(); document.settle();
      auto *username = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-username"));
      auto *password = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-password"));
      if (!username || !password) return 2;
      username->Focus();
      check(active_client && active_client->input, "RmlUi activates its OS text input context");
      if (!active_client) return 1;
      username->SetValue("A😀Z");
      active_client->input->SetSelectionRange(1, 2);
      check(NSEqualRanges([active_client selectedRange], NSMakeRange(1, 2)), "OS selection counts UTF-16 units");
      [active_client insertText:@"é" replacementRange:NSMakeRange(1, 2)];
      check(username->GetValue() == "AéZ", "Composed accent replaces a complete supplementary character");
      username->SetValue("cafe");
      active_client->input->SetSelectionRange(3, 4);
      [active_client setMarkedText:@"é" selectedRange:NSMakeRange(1, 0) replacementRange:NSMakeRange(NSNotFound, 0)];
      check(username->GetValue() == "café" && [active_client hasMarkedText], "Marked accent remains in RmlUi's editor");
      [active_client setMarkedText:@"かな" selectedRange:NSMakeRange(2, 0) replacementRange:NSMakeRange(NSNotFound, 0)];
      check(username->GetValue() == "cafかな", "Updating composition replaces the previous marked range");
      [active_client insertText:@"仮名" replacementRange:NSMakeRange(NSNotFound, 0)];
      check(username->GetValue() == "caf仮名" && ![active_client hasMarkedText], "Commit replaces marked text exactly once");
      active_client->input->SetCursorPosition(5);
      [active_client setMarkedText:@"x" selectedRange:NSMakeRange(1, 0) replacementRange:NSMakeRange(NSNotFound, 0)];
      [active_client doCommandBySelector:@selector(cancelOperation:)];
      check(username->GetValue() == "caf仮名" && ![active_client hasMarkedText], "Cancel discards only uncommitted composition");
      password->Focus();
      password->SetValue("páss😀");
      check([active_client attributedSubstringForProposedRange:NSMakeRange(0, 2) actualRange:nullptr] == nil,
            "Masked password does not expose surrounding text to OS services");
      rib::EventQueue events;
      rib::Lists lists(document, events);
      rib::Overlays overlays(document);
      rib::Achievements achievements(document, lists, events, overlays);
      session.status = RIB_ACHIEVEMENTS_SIGNED_OUT; ++session.revision;
      achievements.bind();
      achievements.handle(rib::Event::account_action(rib::AccountAction::Open));
      password->Focus(); password->SetValue("páss"); document.settle(); password->SetSelectionRange(4, 4);
      for (int toggle = 0; toggle < 2; ++toggle) {
         achievements.handle(rib::Event::account_action(rib::AccountAction::RevealPassword));
         // On Show/Hide we replace the field with a copy of the other type.
         password = dynamic_cast<Rml::ElementFormControlInput*>(document.root()->GetElementById("achievement-password"));
         check(active_client && active_client->input, "Show/Hide keeps the focused native text context active");
         if (active_client && active_client->input)
            [active_client insertText:@"é" replacementRange:NSMakeRange(NSNotFound, 0)];
      }
      check(password->GetValue() == "pásséé", "Accented typing and caret survive both password visibility changes");
      auto *input = active_client ? active_client->input : nullptr;
      password->Blur();
      check(active_client == nil, "Blur releases OS composition ownership");
      username->Focus();
      check(active_client && active_client->input != input, "Refocusing selects the new editor context");
      document.shutdown();
      check(active_client == nil, "Document destruction drops the OS text context");
      std::printf("text composition: %d failures\n", failures);
      return failures ? 1 : 0;
   }
}
