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
 * In each context we also make a picture of a word and draw it inside one
 * menu frame, with the unpack row length still at the value from glcore,
 * as when we make the picture of a glyph in RmlUi during a menu frame.
 *
 * We make the context in the code for each platform (gl_context.h). */

#include "gl_context.h"
#include "test_arguments.h"

#include "rmlui/render/platform.h"
#include "rmlui/render/rmlui_gl.h"
#include "third_party/lodepng.h"

#include <streams/file_stream.h>

#include <algorithm>
#include <cstdio>
#include <cstdlib>
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

/* The word MENU in a three-by-five font, light on dark, one row of the
 * picture per string. For a line of text, GenerateTexture receives a
 * picture like this one from RmlUi. */
const char *const word[] = {
   "..................",
   ".#.#.###.#..#.#.#.",
   ".###.#...##.#.#.#.",
   ".###.##..#.##.#.#.",
   ".#.#.#...#..#.#.#.",
   ".#.#.###.#..#.###.",
   "..................",
};
constexpr int word_width = 18;
constexpr int word_height = 7;
/* The row length after the upload of a Mega Drive frame under glcore, which
 * is the frame's pitch in pixels. */
constexpr GLint frame_row_length = 320;

void word_pixel(int x, int y, unsigned char *rgba)
{
   const bool ink = word[y][x] == '#';
   rgba[0] = ink ? 250 : 20;
   rgba[1] = ink ? 240 : 30;
   rgba[2] = ink ? 90 : 110;
   rgba[3] = 255;
}

/* The drawn picture, eight times larger, for a person to look at. */
void write_enlarged(const std::string& path, const std::vector<unsigned char>& rgba, int width, int height)
{
   const int scale = 8;
   std::vector<unsigned char> large((size_t)width * scale * height * scale * 4);
   for (int y = 0; y < height * scale; y++)
      for (int x = 0; x < width * scale; x++)
         for (int c = 0; c < 4; c++)
            large[((size_t)y * width * scale + x) * 4 + c] =
               rgba[((size_t)(y / scale) * width + x / scale) * 4 + c];
   std::vector<unsigned char> encoded;
   if (lodepng::encode(encoded, large, width * scale, height * scale) == 0)
      filestream_write_file(path.c_str(), encoded.data(), (int64_t)encoded.size());
}

struct Text
{
   bool as_given = false;
   bool state_restored = false;
};

/* In RmlUi a glyph texture is made the first time a piece of text is drawn,
 * inside the menu's frame, after the game's frame has run. With glcore each
 * frame is uploaded with GL_UNPACK_ROW_LENGTH at the frame's pitch, and the
 * value stays there, so a glyph read with that stride would be garbled. We
 * make and draw the picture inside one frame with that row length still set,
 * read it back and compare it with the input, and write the drawn picture
 * beside `picture` so that a person can look at it. */
Text draws_text_as_given(bool core, const char *name, const std::string& picture)
{
   Text result;
   auto renderer = rib_menu_renderer(core);
   renderer->SetViewport(word_width, word_height);
   /* With a row length wider than the picture, a read goes past its end, so
    * we put the picture at the start of a buffer long enough for that stride. */
   std::vector<unsigned char> source((size_t)frame_row_length * word_height * 4);
   for (int y = 0; y < word_height; y++)
      for (int x = 0; x < word_width; x++)
         word_pixel(x, y, &source[((size_t)y * word_width + x) * 4]);
   const Rml::ColourbPremultiplied white(255, 255, 255, 255);
   Rml::Vertex vertices[4] = {
      {{0, 0}, white, {0, 0}},
      {{(float)word_width, 0}, white, {1, 0}},
      {{(float)word_width, (float)word_height}, white, {1, 1}},
      {{0, (float)word_height}, white, {0, 1}},
   };
   const int indices[6] = {0, 1, 2, 0, 2, 3};
   Rml::CompiledGeometryHandle geometry = renderer->CompileGeometry(
         Rml::Span<const Rml::Vertex>(vertices, 4),
         Rml::Span<const int>(indices, 6));
   GLuint vao = 0;
   if (core)
   {
      glGenVertexArrays(1, &vao);
      glBindVertexArray(vao);
   }
   glClearColor(0, 0, 0, 1);
   glClear(GL_COLOR_BUFFER_BIT);
   glPixelStorei(GL_UNPACK_ROW_LENGTH, frame_row_length);
   renderer->BeginFrame();
   const Rml::TextureHandle texture = renderer->GenerateTexture(
         Rml::Span<const Rml::byte>(source.data(), (size_t)word_width * word_height * 4),
         Rml::Vector2i(word_width, word_height));
   renderer->RenderGeometry(geometry, Rml::Vector2f(0, 0), texture);
   renderer->EndFrame();
   GLint row_length = 0;
   glGetIntegerv(GL_UNPACK_ROW_LENGTH, &row_length);
   result.state_restored = row_length == frame_row_length;
   glPixelStorei(GL_UNPACK_ROW_LENGTH, 0);
   renderer->ReleaseTexture(texture);
   renderer->ReleaseGeometry(geometry);
   if (core)
      glDeleteVertexArrays(1, &vao);

   glFinish();
   std::vector<unsigned char> drawn((size_t)word_width * word_height * 4);
   glPixelStorei(GL_PACK_ALIGNMENT, 1);
   glPixelStorei(GL_PACK_ROW_LENGTH, 0);
   glReadPixels(0, 0, word_width, word_height, GL_RGBA, GL_UNSIGNED_BYTE, drawn.data());
   /* Read back from the bottom row up; the picture's first row is the top. */
   std::vector<unsigned char> upright(drawn.size());
   for (int y = 0; y < word_height; y++)
      std::copy_n(&drawn[(size_t)(word_height - 1 - y) * word_width * 4], word_width * 4,
            &upright[(size_t)y * word_width * 4]);
   result.as_given = true;
   for (int y = 0; y < word_height; y++)
      for (int x = 0; x < word_width; x++)
      {
         unsigned char wanted[4];
         word_pixel(x, y, wanted);
         const unsigned char *got = &upright[((size_t)y * word_width + x) * 4];
         for (int c = 0; c < 3; c++)
            if (std::abs((int)got[c] - (int)wanted[c]) > 2)
               result.as_given = false;
      }
   const std::string folder = picture.substr(0, picture.find_last_of("/\\") + 1);
   write_enlarged(folder + "text-" + name + ".png", upright, word_width, word_height);
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
   OffscreenGl *context = offscreen_gl_create(core);
   if (!context)
   {
      std::printf("FAIL %s context could not be created\n", name);
      return 1;
   }
   const Draw drawn = draw(core, leftover);
   const bool loaded = loads_picture(core, picture);
   /* Once per context, because this check is not about the leftover arrays. */
   const Text text = leftover == Leftover::none ? draws_text_as_given(core, name, picture) : Text{true, true};
   offscreen_gl_release(context);
   if (!text.as_given)
   {
      std::printf("FAIL %s context menu text made after a frame left GL_UNPACK_ROW_LENGTH at %d was not drawn as given\n",
            name, (int)frame_row_length);
      return 1;
   }
   if (!text.state_restored)
   {
      std::printf("FAIL %s context menu frame did not put back the game's GL_UNPACK_ROW_LENGTH\n", name);
      return 1;
   }
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
   Utf8Arguments arguments(argc, argv);
   if (arguments.argc() != 2)
   {
      std::printf("usage: menu_core_gl PICTURE\n");
      return 2;
   }
   const std::string picture = arguments.argv()[1];
   offscreen_gl_start();
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
