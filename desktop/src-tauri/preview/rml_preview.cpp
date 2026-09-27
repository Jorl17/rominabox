/* The builder's menu preview. We draw a composed menu into a picture with
 * the player's own RmlUi renderer, off screen, with nothing shown.
 *
 *   rml-preview DOCUMENT OUTPUT WIDTH HEIGHT [--set ID:NAME=VALUE]...
 *
 * DOCUMENT is the menu we composed in the exporter (menu::render_preview),
 * beside its design.cfg, style sheet, fonts and pictures. We write OUTPUT as
 * a PNG of WIDTH by HEIGHT. We read and write files through libretro's file
 * layer, as in the player, so every path is UTF-8 on every
 * platform.
 *
 * With each --set we change the element with that id before we draw it,
 * through the same calls as at runtime in the player and RmlUi. NAME `class`
 * sets a class, `pseudo` a pseudo-class, `text` the element's words, and any
 * other NAME a property. In the picture tests we draw a menu's states this way
 * (scripts/fixtures/menu-states.json). */

#include "gl_context.h"

#include "rmlui/declarations.h"
#include "rmlui/elements.hpp"
#include "rmlui/file_layer.hpp"
#include "rmlui/render/platform.h"
#include "rmlui/render/rmlui_gl.h"
#include "third_party/lodepng.h"

#include <RmlUi/Core.h>
#include <file/config_file.h>
#include <streams/file_stream.h>

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <memory>
#include <sstream>
#include <string>
#include <vector>

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

/* The fonts declared in the design, read from design.cfg as in the player. */
std::vector<std::string> design_fonts(const std::filesystem::path& folder)
{
   const std::string path = (folder / rib::files::Design).u8string();
   std::vector<std::string> fonts;
   config_file_t *config = config_file_new_from_path_to_string(path.c_str());
   if (!config)
      return fonts;
   char *value = nullptr;
   if (config_get_string(config, rib::keys::Fonts, &value) && value)
   {
      std::istringstream words(value);
      for (std::string word; words >> word;)
         fonts.push_back((folder / std::filesystem::u8path(word)).u8string());
   }
   free(value);
   config_file_free(config);
   return fonts;
}

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
      rib::write_text(element, change.value);
   else if (!element->SetProperty(change.name, change.value))
   {
      std::fprintf(stderr, "%s does not take %s: %s\n", change.id.c_str(), change.name.c_str(),
            change.value.c_str());
      return false;
   }
   return true;
}

int render(const std::string& document_path, const std::string& output, int width, int height,
      const std::vector<Change>& changes)
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
   /* The design's fonts, loaded as in the player, with the first as the
    * fallback for any glyph missing from a face. */
   const auto folder = std::filesystem::u8path(document_path).parent_path();
   const std::vector<std::string> fonts = design_fonts(folder);
   if (!failed && fonts.empty())
   {
      std::fprintf(stderr, "the design declares no fonts\n");
      failed = 4;
   }
   for (size_t index = 0; !failed && index < fonts.size(); ++index)
      if (!Rml::LoadFontFace(fonts[index], false)
            || (index == 0 && !Rml::LoadFontFace(fonts[index], true)))
      {
         std::fprintf(stderr, "could not load the font %s\n", fonts[index].c_str());
         failed = 4;
      }
   Rml::Context *context = failed ? nullptr
         : Rml::CreateContext("preview", Rml::Vector2i(width, height));
   /* In RmlUi, a document's folder, from which we read its pictures, is its
    * path up to the last forward slash. A Windows path with only backslashes
    * has no folder there, and no picture would be read. */
   const std::string source = std::filesystem::u8path(document_path).generic_u8string();
   Rml::ElementDocument *document = context ? context->LoadDocument(source) : nullptr;
   if (!failed && !document)
   {
      std::fprintf(stderr, "could not load %s\n", document_path.c_str());
      failed = 4;
   }
   for (const Change& change : changes)
      if (!failed && !apply(document, change))
         failed = 2;

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

int run(const std::vector<std::string>& arguments)
{
   std::vector<Change> changes;
   bool understood = arguments.size() >= 5 && (arguments.size() - 5) % 2 == 0;
   for (size_t index = 5; understood && index < arguments.size(); index += 2)
   {
      Change change;
      understood = arguments[index] == "--set" && parse_change(arguments[index + 1], change);
      changes.push_back(change);
   }
   if (!understood)
   {
      std::fprintf(stderr, "usage: rml-preview DOCUMENT OUTPUT WIDTH HEIGHT [--set ID:NAME=VALUE]...\n");
      return 2;
   }
   return render(arguments[1], arguments[2], std::atoi(arguments[3].c_str()),
         std::atoi(arguments[4].c_str()), changes);
}

}

/* The arguments as UTF-8. On Windows, main receives them in the ANSI code
 * page, which cannot represent every folder name, so we use the wide entry. */
#if defined(_WIN32)
#include <windows.h>

int wmain(int argc, wchar_t **argv)
{
   std::vector<std::string> arguments;
   for (int index = 0; index < argc; index++)
   {
      const int size = WideCharToMultiByte(CP_UTF8, 0, argv[index], -1, nullptr, 0, nullptr, nullptr);
      std::string argument(size > 0 ? (size_t)size - 1 : 0, '\0');
      if (size > 1)
         WideCharToMultiByte(CP_UTF8, 0, argv[index], -1, &argument[0], size, nullptr, nullptr);
      arguments.push_back(argument);
   }
   return run(arguments);
}
#elif defined(__APPLE__) || defined(__unix__)
int main(int argc, char **argv)
{
   return run(std::vector<std::string>(argv, argv + argc));
}
#else
#error "rml-preview has no entry declared for this platform"
#endif
