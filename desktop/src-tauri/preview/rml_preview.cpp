/* The builder's menu preview. We draw a composed menu into a picture with
 * the player's own RmlUi renderer, off screen, with nothing shown.
 *
 *   rml-preview DOCUMENT OUTPUT WIDTH HEIGHT [--screen ID] [--set ID:NAME=VALUE]...
 *
 * DOCUMENT is the menu we composed in the exporter (menu::render_preview),
 * beside its design.cfg, style sheet, fonts and pictures. We write OUTPUT as
 * a PNG of WIDTH by HEIGHT. We read and write files through libretro's file
 * layer, as in the player, so every path is UTF-8 on every
 * platform.
 *
 * With each --set we change the element with that id before we draw it,
 * through the same calls as at runtime in the player and RmlUi. NAME `class`
 * sets a class, `pseudo` a pseudo-class, `text` the element's content as
 * markup, as we write it inside the element in the exporter (an options
 * button's label is a span), and any other NAME a property. In the picture
 * tests we draw a menu's states this way (scripts/fixtures/menu-states.json).
 *
 * With --screen we show the design's screen of that id as in the player
 * (screen_display.hpp), with its panel alone, its heading and its footer.
 * Without it, we draw the document as it was composed, showing Pause.
 *
 * We read the design as in the player (load_design), with its fonts, its
 * words and its screens. Once we have shown the screen and made the changes,
 * we split every list into pages by the player's own rules (paging.hpp),
 * from its first page, as in the menu when it loads. */

#include "rml_preview.h"
#include "gl_context.h"

#include "rmlui/declarations.h"
#include "rmlui/document_contract.hpp"
#include "rmlui/elements.hpp"
#include "rmlui/file_layer.hpp"
#include "rmlui/paging.hpp"
#include "rmlui/screen_display.hpp"
#include "rmlui/words.hpp"
#include "rmlui/render/platform.h"
#include "rmlui/render/rmlui_gl.h"
#include "third_party/lodepng.h"

#include <RmlUi/Core.h>
#include <streams/file_stream.h>

#include <algorithm>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <memory>
#include <string>
#include <vector>

/* The messages about a design from the design reader, which RetroArch would
 * log. We write them to the preview's error output. */
static void log_line(const char *format, va_list args)
{
   std::vfprintf(stderr, format, args);
}
extern "C" void RARCH_LOG(const char *format, ...)
{
   va_list args;
   va_start(args, format);
   log_line(format, args);
   va_end(args);
}
extern "C" void RARCH_WARN(const char *format, ...)
{
   va_list args;
   va_start(args, format);
   log_line(format, args);
   va_end(args);
}
extern "C" void RARCH_ERR(const char *format, ...)
{
   va_list args;
   va_start(args, format);
   log_line(format, args);
   va_end(args);
}

namespace {

struct Quiet : Rml::SystemInterface
{
   double GetElapsedTime() override { return 0.0; }
   bool LogMessage(Rml::Log::Type type, const Rml::String& message) override
   {
      if (type <= Rml::Log::LT_WARNING)
         std::fprintf(stderr, "[RmlUi] %s\n", message.c_str());
      return true;
   }
   void JoinPath(Rml::String& output, const Rml::String& document_path,
         const Rml::String& path) override
   {
      rib::join_menu_path(output, document_path, path);
   }
};


/* One --set: an element, what to change on it and the value. */
struct Change
{
   std::string id, name, value;
};

bool parse_change(const std::string& text, Change& change)
{
   const size_t colon = text.find(':');
   const size_t equals = colon == std::string::npos ? colon : text.find('=', colon);
   if (colon == 0 || equals == std::string::npos || equals == colon + 1)
      return false;
   change = {text.substr(0, colon), text.substr(colon + 1, equals - colon - 1), text.substr(equals + 1)};
   return true;
}

bool apply(Rml::ElementDocument *document, const Change& change)
{
   Rml::Element *element = document->GetElementById(change.id);
   if (!element)
   {
      std::fprintf(stderr, "nothing named %s in the document\n", change.id.c_str());
      return false;
   }
   if (change.name == "class")
      element->SetClass(change.value, true);
   else if (change.name == "pseudo")
      element->SetPseudoClass(change.value, true);
   else if (change.name == "text")
      element->SetInnerRML(change.value);
   else if (!element->SetProperty(change.name, change.value))
   {
      std::fprintf(stderr, "%s does not take %s: %s\n", change.id.c_str(), change.name.c_str(),
            change.value.c_str());
      return false;
   }
   return true;
}

int render(const std::string& document_path, const std::string& output, int width, int height,
      const std::string& screen, const std::vector<Change>& changes)
{
   if (width <= 0 || height <= 0)
   {
      std::fprintf(stderr, "the picture needs a width and a height\n");
      return 2;
   }
   offscreen_gl_start();
   OffscreenGl *gl = offscreen_gl_create(false);
   if (!gl)
   {
      std::fprintf(stderr, "no OpenGL context to draw in\n");
      return 3;
   }
   auto renderer = rib_menu_renderer(false);
   Quiet system;
   rib::FileLayer files;
   Rml::SetSystemInterface(&system);
   Rml::SetFileInterface(&files);
   Rml::SetRenderInterface(renderer.get());
   int failed = 0;
   if (!Rml::Initialise())
      failed = 3;
   /* The design, read as in the player: its words, and its fonts, with the
    * first as the fallback for any glyph missing from a face. */
   const auto folder = std::filesystem::u8path(document_path).parent_path();
   const rib::DesignDeclarations design = rib::load_design(folder.u8string().c_str());
   rib::use_words(design.words);
   std::vector<std::string> fonts;
   for (const std::string& font : design.fonts)
      fonts.push_back((folder / std::filesystem::u8path(font)).u8string());
   if (!failed && fonts.empty())
   {
      std::fprintf(stderr, "the design declares no fonts\n");
      failed = 4;
   }
   for (size_t index = 0; !failed && index < fonts.size(); ++index)
      if (!Rml::LoadFontFace(rib::to_rml_path(fonts[index]), false)
            || (index == 0 && !Rml::LoadFontFace(rib::to_rml_path(fonts[index]), true)))
      {
         std::fprintf(stderr, "could not load the font %s\n", fonts[index].c_str());
         failed = 4;
      }
   Rml::Context *context = failed ? nullptr
         : Rml::CreateContext("preview", Rml::Vector2i(width, height));
   /* We scale the design's canvas to the picture, as we scale it to the
    * window in the player. */
   if (context)
      context->SetDensityIndependentPixelRatio(rib::document_contract::canvas_density(width, height));
   /* In RmlUi, a document's folder, from which we read its pictures, is its
    * path up to the last forward slash. A Windows path with only backslashes
    * has no folder there, and no picture would be read. A '?' would also cut
    * the path there (file_layer.hpp). */
   const std::string source = rib::to_rml_path(std::filesystem::u8path(document_path).generic_u8string());
   Rml::ElementDocument *document = context ? context->LoadDocument(source) : nullptr;
   if (!failed && !document)
   {
      std::fprintf(stderr, "could not load %s\n", document_path.c_str());
      failed = 4;
   }
   if (!failed && !screen.empty())
   {
      const auto& screens = design.screens;
      const auto wanted = std::find_if(screens.begin(), screens.end(),
            [&](const rib::ScreenDeclaration& declared) { return declared.id == screen; });
      if (wanted == screens.end())
      {
         std::fprintf(stderr, "the design declares no screen %s\n", screen.c_str());
         failed = 2;
      }
      else
         rib::display_screen(document, screens, *wanted);
   }
   for (const Change& change : changes)
      if (!failed && !apply(document, change))
         failed = 2;
   if (!failed)
      rib::paging::split_all(document);
   /* The footer's hint, written as in the player (Screens::set_footer_hint).
    * We make the key in brackets an element of its own. We write it so for a
    * footer that still has the words it was composed or set with. A footer
    * written when we showed a screen already contains its key. */
   if (!failed)
      if (Rml::Element *hint = document->GetElementById(rib::document_contract::FooterHint))
         if (!rib::find_class(hint, rib::document_contract::HintKey))
            rib::write_hint(hint, rib::words_of(hint));

   /* We draw into a framebuffer of our own, because a window that is never
    * shown has no pixels to read back. */
   GLuint framebuffer = 0, colour = 0;
   std::vector<unsigned char> encoded;
   if (!failed)
   {
      document->Show();
      context->Update();
      glGenFramebuffers(1, &framebuffer);
      glBindFramebuffer(GL_FRAMEBUFFER, framebuffer);
      glGenTextures(1, &colour);
      glBindTexture(GL_TEXTURE_2D, colour);
      glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, width, height, 0, GL_RGBA, GL_UNSIGNED_BYTE, nullptr);
      glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, colour, 0);
      if (glCheckFramebufferStatus(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE)
      {
         std::fprintf(stderr, "no framebuffer to draw into\n");
         failed = 5;
      }
   }
   if (!failed)
   {
      glViewport(0, 0, width, height);
      glClearColor(0, 0, 0, 1);
      glClear(GL_COLOR_BUFFER_BIT);
      renderer->SetViewport(width, height);
      renderer->BeginFrame();
      context->Render();
      renderer->EndFrame();
      glFinish();
      std::vector<unsigned char> pixels((size_t)width * height * 4);
      glPixelStorei(GL_PACK_ALIGNMENT, 1);
      glReadPixels(0, 0, width, height, GL_RGBA, GL_UNSIGNED_BYTE, pixels.data());
      /* OpenGL's rows go upwards, and a picture's rows go downwards. The
       * picture is opaque, like the screen it represents. */
      std::vector<unsigned char> picture(pixels.size());
      const size_t row = (size_t)width * 4;
      for (int y = 0; y < height; y++)
         std::copy_n(pixels.begin() + (size_t)(height - 1 - y) * row, row,
               picture.begin() + (size_t)y * row);
      for (size_t alpha = 3; alpha < picture.size(); alpha += 4)
         picture[alpha] = 255;
      if (lodepng::encode(encoded, picture, (unsigned)width, (unsigned)height) != 0
            || !filestream_write_file(output.c_str(), encoded.data(), (int64_t)encoded.size()))
      {
         std::fprintf(stderr, "could not write %s\n", output.c_str());
         failed = 6;
      }
   }
   if (framebuffer)
   {
      glBindFramebuffer(GL_FRAMEBUFFER, 0);
      glDeleteFramebuffers(1, &framebuffer);
      glDeleteTextures(1, &colour);
   }
   if (document)
      document->Close();
   Rml::Shutdown();
   renderer.reset();
   offscreen_gl_release(gl);
   return failed;
}

}

int run(const std::vector<std::string>& arguments)
{
   std::vector<Change> changes;
   std::string screen;
   bool understood = arguments.size() >= 5 && (arguments.size() - 5) % 2 == 0;
   for (size_t index = 5; understood && index < arguments.size(); index += 2)
   {
      if (arguments[index] == "--screen" && screen.empty())
      {
         screen = arguments[index + 1];
         understood = !screen.empty();
         continue;
      }
      Change change;
      understood = arguments[index] == "--set" && parse_change(arguments[index + 1], change);
      changes.push_back(change);
   }
   if (!understood)
   {
      std::fprintf(stderr, "usage: rml-preview DOCUMENT OUTPUT WIDTH HEIGHT [--screen ID] [--set ID:NAME=VALUE]...\n"
            "NAME is class, pseudo, text (the element's content, as markup) or a property\n");
      return 2;
   }
   return render(arguments[1], arguments[2], std::atoi(arguments[3].c_str()),
         std::atoi(arguments[4].c_str()), screen, changes);
}
