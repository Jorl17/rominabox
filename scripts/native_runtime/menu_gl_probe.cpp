/* Draw one rectangle with the menu's renderer into a core profile and
 * into a legacy context, and load one picture in each. A core profile has
 * no client arrays, so with the GL2 backend the framebuffer stays clear.
 *
 * We draw into the legacy context twice: once clean, and once with the
 * generic vertex attribute arrays still enabled after the RetroArch GLSL
 * shader path. On drivers where attribute 0 aliases the fixed-function
 * vertex position (NVIDIA's compatibility profile on Windows), those arrays
 * would replace the menu's positions and the whole menu would be invisible.
 *
 * The arguments and the context come from the code for each platform
 * (menu_gl_platform.h). */

#include "menu_gl_platform.h"

#include "rmlui/render/platform.h"
#include "rmlui/render/rmlui_gl.h"
#include "third_party/lodepng.h"

#include <streams/file_stream.h>

#include <cstdio>
#include <string>
#include <vector>

#if defined(__APPLE__)
/* The legacy header declares neither, and the core profile exports both. */
extern "C" void glGenVertexArrays(GLsizei n, GLuint *arrays);
extern "C" void glDeleteVertexArrays(GLsizei n, const GLuint *arrays);
#elif defined(_WIN32)
/* They are declared in the loader in platform.h. */
#else
#error "the menu GL probe has no GL entry points declared for this platform"
#endif

namespace {

enum class Leftover
{
   none,
   /* Arrays 0 and 1 enabled, as after the RetroArch shader path. */
   shader_attributes,
};

constexpr GLuint leftover_arrays = 2;

struct Draw
{
   bool red = false;
   bool state_restored = true;
   unsigned char rgba[4] = {};
};

Draw draw(bool core, Leftover leftover)
{
   Draw result;
   const int width = 64;
   const int height = 64;
   auto renderer = rib_menu_renderer(core);
   renderer->SetViewport(width, height);

   const Rml::ColourbPremultiplied red(255, 0, 0, 255);
   Rml::Vertex vertices[4] = {
      {{0, 0}, red, {0, 0}},
      {{(float)width, 0}, red, {1, 0}},
      {{(float)width, (float)height}, red, {1, 1}},
      {{0, (float)height}, red, {0, 1}},
   };
   const int indices[6] = {0, 1, 2, 0, 2, 3};
   Rml::CompiledGeometryHandle geometry = renderer->CompileGeometry(
         Rml::Span<const Rml::Vertex>(vertices, 4),
         Rml::Span<const int>(indices, 6));

   glClearColor(0, 0, 0, 1);
   glClear(GL_COLOR_BUFFER_BIT);
   GLuint vao = 0;
   if (core)
   {
      glGenVertexArrays(1, &vao);
      glBindVertexArray(vao);
   }
   /* Every vertex at the origin, so if an array replaces the menu's
    * positions, every triangle has no area. */
   static const GLfloat origin[4 * 2] = {};
   if (leftover == Leftover::shader_attributes)
      for (GLuint i = 0; i < leftover_arrays; i++)
      {
         glVertexAttribPointer(i, 2, GL_FLOAT, GL_FALSE, 0, origin);
         glEnableVertexAttribArray(i);
      }
   renderer->BeginFrame();
   renderer->RenderGeometry(geometry, Rml::Vector2f(0, 0), 0);
   renderer->EndFrame();
   if (core)
   {
      GLint bound = 0;
      glGetIntegerv(GL_VERTEX_ARRAY_BINDING, &bound);
      result.state_restored = bound == (GLint)vao;
      glDeleteVertexArrays(1, &vao);
   }
   if (leftover == Leftover::shader_attributes)
      for (GLuint i = 0; i < leftover_arrays; i++)
      {
         GLint enabled = 0;
         glGetVertexAttribiv(i, GL_VERTEX_ATTRIB_ARRAY_ENABLED, &enabled);
         result.state_restored = result.state_restored && enabled;
         glDisableVertexAttribArray(i);
      }
   renderer->ReleaseGeometry(geometry);

   glFinish();
   glPixelStorei(GL_PACK_ALIGNMENT, 1);
   glReadPixels(width / 2, height / 2, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, result.rgba);
   result.red = result.rgba[0] > 200 && result.rgba[1] < 40 && result.rgba[2] < 40;
   return result;
}

/* Read a picture that the menu shows, such as a slot picture, through
 * libretro's file layer, decode it from memory and return its size. The
 * folder name is like a game's data folder under a non-ASCII home folder. */
bool loads_picture(bool core, const std::string& path)
{
   auto renderer = rib_menu_renderer(core);
   Rml::Vector2i dimensions;
   const Rml::TextureHandle texture = renderer->LoadTexture(dimensions, path);
   if (texture)
      renderer->ReleaseTexture(texture);
   return texture && dimensions.x == 3 && dimensions.y == 2;
}

int check(bool core, Leftover leftover, const char *name, const std::string& picture)
{
   ProbeContext *context = probe_context_create(core);
   if (!context)
   {
      std::printf("FAIL %s context could not be created\n", name);
      return 1;
   }
   const Draw drawn = draw(core, leftover);
   const bool loaded = loads_picture(core, picture);
   probe_context_release(context);
   if (!loaded)
   {
      std::printf("FAIL %s context could not load %s as a 3x2 picture\n", name, picture.c_str());
      return 1;
   }
   if (!drawn.state_restored)
   {
      std::printf(core ? "FAIL %s context menu draw did not restore the VAO\n"
                       : "FAIL %s context menu draw did not restore the enabled attribute arrays\n",
            name);
      return 1;
   }
   if (drawn.red)
   {
      std::printf("ok %s context menu drew\n", name);
      return 0;
   }
   std::printf("FAIL %s context menu draw left the framebuffer clear (pixel %u %u %u %u)\n",
         name, drawn.rgba[0], drawn.rgba[1], drawn.rgba[2], drawn.rgba[3]);
   return 1;
}

}

int main(int argc, char **argv)
{
   const std::vector<std::string> arguments = probe_platform_start(argc, argv);
   if (arguments.size() != 2)
   {
      std::printf("usage: menu_core_gl PICTURE\n");
      return 2;
   }
   const std::string& picture = arguments[1];
   /* Write three by two opaque pixels at the given path, through the file
    * layer that we use in the menu. */
   std::vector<unsigned char> encoded;
   const std::vector<unsigned char> pixels(3 * 2 * 4, 255);
   if (lodepng::encode(encoded, pixels, 3, 2) != 0
         || !filestream_write_file(picture.c_str(), encoded.data(), (int64_t)encoded.size()))
   {
      std::printf("FAIL could not write %s\n", picture.c_str());
      return 1;
   }
   int failed = 0;
   failed += check(true, Leftover::none, "core", picture);
   failed += check(false, Leftover::none, "legacy", picture);
   failed += check(false, Leftover::shader_attributes, "legacy after RetroArch's shaders", picture);
   return failed == 0 ? 0 : 1;
}
