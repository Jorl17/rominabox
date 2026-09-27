/* The context of the menu GL probe on macOS: NSOpenGL on a borderless window
 * that we never order front. With GL3 we draw onto the default framebuffer,
 * and a context without a drawable has no such framebuffer to read. */

#include "menu_gl_platform.h"

#import <Cocoa/Cocoa.h>

struct ProbeContext
{
   NSOpenGLContext *context;
   NSWindow *window;
};

void probe_platform_start()
{
   @autoreleasepool
   {
      NSApplication *app = [NSApplication sharedApplication];
      [app setActivationPolicy:NSApplicationActivationPolicyProhibited];
   }
}

ProbeContext *probe_context_create(bool core)
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
      return new ProbeContext{context, window};
   }
}

void probe_context_release(ProbeContext *context)
{
   @autoreleasepool
   {
      [NSOpenGLContext clearCurrentContext];
      delete context;
   }
}
