/* A game's folder can have any name that a title can, for example "Who
 * Wants a Game?". RmlUi keeps the path of a document as a URL and takes the
 * folder of an image from the part before a "?", which would point the
 * menu's pictures at the folder above. Here we load the menu from such a
 * folder and check that the footer mark has the size from the style sheet
 * next to the document and that every picture we request exists. */
#include "rmlui/view.hpp"
#include "rmlui/file_layer.hpp"
#include <cstdio>
#include <string>
#include <vector>

int test_menu_named_folder(const char *assets, const std::vector<std::string>& fonts,
      const rib_controls_catalog& controls)
{
   rib::View view;
   int failures = 0;
   auto check = [&](bool holds, const char *what) {
      if (!holds)
      {
         std::fprintf(stderr, "FAIL %s\n", what);
         ++failures;
      }
   };
   // Every path round-trips, whatever it contains, and RmlUi gets no '?'.
   for (const std::string path : {"/Games/Who Wants a Game?.app", "C:/100% Games/a%3Fb?", "plain"})
   {
      check(rib::from_rml_path(rib::to_rml_path(path)) == path, "a path comes back from RmlUi as it went in");
      check(rib::to_rml_path(path).find('?') == std::string::npos, "RmlUi is given no '?'");
   }
   if (!view.initialize(assets, fonts, 960, 600, false, controls))
   {
      std::fprintf(stderr, "FAIL could not init RmlUi from %s\n", assets);
      return 1;
   }
   view.render(960, 600);
   Rml::Element *mark = view.document.root()->GetElementById("brand-mark");
   check(mark != nullptr, "the footer has its mark");
   if (mark)
   {
      const Rml::Vector2f size = mark->GetBox().GetSize();
      check(size.x == 26.0f && size.y == 18.0f,
            "the style sheet beside the document sizes the mark");
   }
   const auto& missing = view.document.missing_pictures();
   check(missing.empty(), "every picture the menu asks for is found beside its document");
   for (const auto& picture : missing)
      std::fprintf(stderr, "  not found: %s\n", picture.c_str());
   view.shutdown();
   if (failures)
   {
      std::fprintf(stderr, "%d check(s) failed\n", failures);
      return 1;
   }
   std::printf("ok\n");
   return 0;
}
