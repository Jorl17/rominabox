/* The off-screen menu context on macOS: NSOpenGL on a borderless window
 * that is never ordered front. With GL3 we composite onto the default
 * framebuffer, and a context with no drawable has none to read. */

#include "gl_context.h"

#import <Cocoa/Cocoa.h>

struct OffscreenGl
{
   NSOpenGLContext *context;
   NSWindow *window;
};

void offscreen_gl_start()
{
   @autoreleasepool
   {
      NSApplication *app = [NSApplication sharedApplication];
      [app setActivationPolicy:NSApplicationActivationPolicyProhibited];
   }
}

OffscreenGl *offscreen_gl_create(bool core)
{
   @autoreleasepool
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
         return nullptr;
      NSOpenGLContext *context = [[NSOpenGLContext alloc] initWithFormat:format shareContext:nil];
      NSWindow *window = [[NSWindow alloc]
            initWithContentRect:NSMakeRect(0, 0, 64, 64)
                      styleMask:NSWindowStyleMaskBorderless
                        backing:NSBackingStoreBuffered
                          defer:NO];
      [context setView:window.contentView];
      [context makeCurrentContext];
      [context update];
      return new OffscreenGl{context, window};
   }
}

void offscreen_gl_release(OffscreenGl *context)
{
   @autoreleasepool
   {
      [NSOpenGLContext clearCurrentContext];
      delete context;
   }
}
