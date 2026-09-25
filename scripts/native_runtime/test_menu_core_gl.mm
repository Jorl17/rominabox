/* Draw one rectangle with the menu's renderer into a core profile and
 * into a legacy context, and load one picture in each. A core profile has
 * no client arrays, so with the GL2 backend the framebuffer stays clear. We
 * never order the window front. GL3 composites onto the default
 * framebuffer, and a context with no drawable has none to read. */

#import <Cocoa/Cocoa.h>
#import <OpenGL/gl.h>

#include "rmlui/render/rmlui_gl.h"
#include "third_party/lodepng.h"

#include <cstdio>
#include <string>
#include <vector>

extern "C" void glGenVertexArrays(GLsizei n, GLuint *arrays);
extern "C" void glDeleteVertexArrays(GLsizei n, const GLuint *arrays);
extern "C" void glBindVertexArray(GLuint array);
#define RIB_GL_VERTEX_ARRAY_BINDING 0x85B5

static NSOpenGLContext *make_context(bool core, NSWindow **window_out)
{
   NSOpenGLPixelFormatAttribute profile = core
         ? NSOpenGLProfileVersion3_2Core
         : NSOpenGLProfileVersionLegacy;
   NSOpenGLPixelFormatAttribute attrs[] = {
      NSOpenGLPFAOpenGLProfile, profile,
      NSOpenGLPFAColorSize, 24,
      NSOpenGLPFAAlphaSize, 8,
      NSOpenGLPFAStencilSize, 8,
      NSOpenGLPFAAccelerated,
      0
   };
   NSOpenGLPixelFormat *format = [[NSOpenGLPixelFormat alloc] initWithAttributes:attrs];
   if (!format)
      return nil;
   NSOpenGLContext *context = [[NSOpenGLContext alloc] initWithFormat:format shareContext:nil];
   NSWindow *window = [[NSWindow alloc]
         initWithContentRect:NSMakeRect(0, 0, 64, 64)
                   styleMask:NSWindowStyleMaskBorderless
                     backing:NSBackingStoreBuffered
                       defer:NO];
   [context setView:window.contentView];
   [context makeCurrentContext];
   [context update];
   *window_out = window;
   return context;
}

static bool draw_is_red(bool core, unsigned char rgba[4], bool *state_restored)
{
   *state_restored = true;
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
   renderer->BeginFrame();
   renderer->RenderGeometry(geometry, Rml::Vector2f(0, 0), 0);
   renderer->EndFrame();
   if (core)
   {
      GLint bound = 0;
      glGetIntegerv(RIB_GL_VERTEX_ARRAY_BINDING, &bound);
      *state_restored = bound == (GLint)vao;
      glDeleteVertexArrays(1, &vao);
   }
   renderer->ReleaseGeometry(geometry);

   glFinish();
   glPixelStorei(GL_PACK_ALIGNMENT, 1);
   unsigned char pixels[4];
   glReadPixels(width / 2, height / 2, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, pixels);
   rgba[0] = pixels[0];
   rgba[1] = pixels[1];
   rgba[2] = pixels[2];
   rgba[3] = pixels[3];
   return pixels[0] > 200 && pixels[1] < 40 && pixels[2] < 40;
}

/* Read a picture that the menu shows, such as a slot picture, through
 * libretro's file layer, decode it from memory and return its size. The
 * folder name is like a game's data folder under a non-ASCII home folder. */
static bool loads_picture(bool core, const std::string& path)
{
   auto renderer = rib_menu_renderer(core);
   Rml::Vector2i dimensions;
   const Rml::TextureHandle texture = renderer->LoadTexture(dimensions, path);
   if (texture)
      renderer->ReleaseTexture(texture);
   return texture && dimensions.x == 3 && dimensions.y == 2;
}

static int check(bool core, const char *name, const std::string& picture)
{
   NSWindow *window = nil;
   NSOpenGLContext *context = make_context(core, &window);
   if (!context)
   {
      std::printf("FAIL %s context could not be created\n", name);
      return 1;
   }
   unsigned char rgba[4] = {};
   bool state_restored = true;
   const bool red = draw_is_red(core, rgba, &state_restored);
   if (!loads_picture(core, picture))
   {
      std::printf("FAIL %s context could not load %s as a 3x2 picture\n", name, picture.c_str());
      [NSOpenGLContext clearCurrentContext];
      return 1;
   }
   if (core && !state_restored)
   {
      std::printf("FAIL core context menu draw did not restore the VAO\n");
      [NSOpenGLContext clearCurrentContext];
      return 1;
   }
   [NSOpenGLContext clearCurrentContext];
   if (red)
   {
      std::printf("ok %s context menu drew\n", name);
      return 0;
   }
   std::printf("FAIL %s context menu draw left the framebuffer clear (pixel %u %u %u %u)\n",
         name, rgba[0], rgba[1], rgba[2], rgba[3]);
   return 1;
}

int main(int argc, char **argv)
{
   if (argc != 2)
   {
      std::printf("usage: menu_core_gl PICTURE\n");
      return 2;
   }
   /* Three by two opaque pixels, written at the location the caller passes. */
   std::vector<unsigned char> encoded;
   const std::vector<unsigned char> pixels(3 * 2 * 4, 255);
   FILE *file = lodepng::encode(encoded, pixels, 3, 2) == 0 ? std::fopen(argv[1], "wb") : nullptr;
   if (!file || std::fwrite(encoded.data(), 1, encoded.size(), file) != encoded.size())
   {
      std::printf("FAIL could not write %s\n", argv[1]);
      return 1;
   }
   std::fclose(file);
   @autoreleasepool
   {
      NSApplication *app = [NSApplication sharedApplication];
      [app setActivationPolicy:NSApplicationActivationPolicyProhibited];
      int failed = 0;
      failed += check(true, "core", argv[1]);
      failed += check(false, "legacy", argv[1]);
      return failed == 0 ? 0 : 1;
   }
}
