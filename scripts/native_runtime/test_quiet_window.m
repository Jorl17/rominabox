#import <AppKit/AppKit.h>
#include <assert.h>
#include <stdio.h>
#include "gfx/drivers_context/cocoa_quiet_window.h"

int main(void)
{
   @autoreleasepool
   {
      [[NSApplication sharedApplication] setActivationPolicy:NSApplicationActivationPolicyProhibited];
      NSWindow *window = [[NSWindow alloc]
            initWithContentRect:NSMakeRect(100, 100, 640, 480)
                      styleMask:NSWindowStyleMaskTitled
                        backing:NSBackingStoreBuffered defer:NO];
      assert(window && [[NSScreen screens] count] > 0);
      const NSRect ordinary = [window frame];

      unsetenv("ROMINABOX_QUIET");
      unsetenv("ROMINABOX_SHOW_WINDOW");
      assert(!rib_session_window_hidden());
      rominabox_prepare_test_window(window);
      assert(NSEqualRects(ordinary, [window frame]));

      setenv("ROMINABOX_QUIET", "1", 1);
      assert(rib_session_window_hidden());
      rominabox_prepare_test_window(window);
      if ([window alphaValue] != 0.0)
      {
         fprintf(stderr, "FAIL quiet window can become visible: alpha %.1f\n", [window alphaValue]);
         return 1;
      }
      NSRect testFrame = [window frame];
      NSRect primary = [[[NSScreen screens] firstObject] visibleFrame];
      if (fabs(NSMidX(testFrame) - NSMidX(primary)) > 1.0
            || fabs(NSMidY(testFrame) - NSMidY(primary)) > 1.0)
      {
         fprintf(stderr, "FAIL quiet drawable is not pinned to the primary display\n");
         return 1;
      }
      if (![window ignoresMouseEvents])
      {
         fprintf(stderr, "FAIL quiet window does not explicitly ignore mouse events\n");
         return 1;
      }
      assert(![window isVisible]);

      setenv("ROMINABOX_SHOW_WINDOW", "1", 1);
      [window setAlphaValue:1.0];
      [window setIgnoresMouseEvents:NO];
      [window setFrame:ordinary display:NO];
      assert(!rib_session_window_hidden());
      rominabox_prepare_test_window(window);
      assert(NSEqualRects(ordinary, [window frame]));
      assert([window alphaValue] == 1.0);
      assert(![window ignoresMouseEvents]);
      puts("quiet window stays transparent; ordinary placement and opacity unchanged");
   }
   return 0;
}
