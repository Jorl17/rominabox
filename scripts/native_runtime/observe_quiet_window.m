// Observe one exported player without creating a window or controlling it.
// clang -fobjc-arc -framework Cocoa -framework ApplicationServices \
//   -o observe_quiet_window observe_quiet_window.m
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#include <libproc.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double monotonic_ms(void)
{
   struct timespec now;
   clock_gettime(CLOCK_MONOTONIC, &now);
   return now.tv_sec * 1000.0 + now.tv_nsec / 1000000.0;
}

static BOOL matches_executable(pid_t pid, const char *wanted)
{
   char process_path[PROC_PIDPATHINFO_MAXSIZE];
   char actual[PATH_MAX];
   return proc_pidpath(pid, process_path, sizeof(process_path)) > 0
         && realpath(process_path, actual)
         && strcmp(actual, wanted) == 0;
}

int main(int argc, const char *argv[])
{
   if (argc != 3)
   {
      fprintf(stderr, "usage: observe_quiet_window EXECUTABLE DURATION_MS\n");
      return 2;
   }
   char executable[PATH_MAX];
   char *end = NULL;
   long duration = strtol(argv[2], &end, 10);
   if (!realpath(argv[1], executable) || !end || *end || duration < 20 || duration > 120000)
   {
      fprintf(stderr, "expected an existing executable and 20..120000 milliseconds\n");
      return 2;
   }
   BOOL saw_window = NO;
   BOOL verified_policy = NO;
   BOOL violation = NO;
   NSString *previous = nil;
   const double deadline = monotonic_ms() + duration;
   do
   {
      @autoreleasepool
      {
         NSMutableString *state = [NSMutableString string];
         CGDirectDisplayID displays[32];
         uint32_t display_count = 0;
         CGError display_error = CGGetOnlineDisplayList(32, displays, &display_count);
         if (display_error != kCGErrorSuccess || display_count == 0)
         {
            fprintf(stderr, "could not enumerate online displays\n");
            return 2;
         }
         CFArrayRef window_list = CGWindowListCopyWindowInfo(kCGWindowListOptionAll, kCGNullWindowID);
         NSArray *windows = CFBridgingRelease(window_list);
         if (!windows)
         {
            fprintf(stderr, "could not enumerate windows\n");
            return 2;
         }
         NSMutableSet<NSNumber *> *matching = [NSMutableSet set];
         for (NSDictionary *window in windows)
         {
            pid_t pid = [window[(id)kCGWindowOwnerPID] intValue];
            if (pid > 0 && matches_executable(pid, executable))
               [matching addObject:@(pid)];
         }
         for (NSNumber *owner in matching)
         {
            pid_t pid = owner.intValue;
            NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
            if (app)
            {
               verified_policy = YES;
               NSApplicationActivationPolicy policy = app.activationPolicy;
               BOOL active = app.active;
               const char *name = policy == NSApplicationActivationPolicyAccessory ? "accessory"
                     : policy == NSApplicationActivationPolicyRegular ? "regular" : "prohibited";
               [state appendFormat:@"pid=%d policy=%s active=%d\n", pid, name, active];
               if (policy == NSApplicationActivationPolicyRegular || active)
                  violation = YES;
            }
            else
               [state appendFormat:@"pid=%d policy=unavailable active=unknown\n", pid];
            for (NSDictionary *window in windows)
            {
               if ([window[(id)kCGWindowOwnerPID] intValue] != pid)
                  continue;
               saw_window = YES;
               CGRect bounds = CGRectZero;
               NSDictionary *encoded = window[(id)kCGWindowBounds];
               BOOL has_bounds = encoded && CGRectMakeWithDictionaryRepresentation(
                     (__bridge CFDictionaryRef)encoded, &bounds);
               NSNumber *alpha_number = window[(id)kCGWindowAlpha];
               NSNumber *onscreen_number = window[(id)kCGWindowIsOnscreen];
               double alpha = alpha_number ? alpha_number.doubleValue : 1.0;
               BOOL onscreen = onscreen_number ? onscreen_number.boolValue : YES;
               BOOL intersects = NO;
               if (has_bounds && !CGRectIsEmpty(bounds))
                  for (uint32_t index = 0; index < display_count; ++index)
                     intersects |= CGRectIntersectsRect(bounds, CGDisplayBounds(displays[index]));
               [state appendFormat:@"  window=%u bounds=(%.0f,%.0f,%.0f,%.0f) alpha=%.3f onscreen=%d intersects=%d\n",
                     [window[(id)kCGWindowNumber] unsignedIntValue], bounds.origin.x,
                     bounds.origin.y, bounds.size.width, bounds.size.height,
                     alpha, onscreen, intersects];
               if (!has_bounds || (alpha > 0 && onscreen && intersects))
                  violation = YES;
            }
         }
         if (![state isEqualToString:previous] && state.length)
         {
            fputs(state.UTF8String, stdout);
            fflush(stdout);
         }
         previous = [state copy];
      }
      if (violation)
         break;
      struct timespec pause = {0, 20 * 1000 * 1000};
      nanosleep(&pause, NULL);
   } while (monotonic_ms() < deadline);
   if (violation)
   {
      fprintf(stderr, "FAIL: player activated or displayed a visible window\n");
      return 1;
   }
   if (!saw_window)
   {
      fprintf(stderr, "FAIL: no window observed for the supplied executable\n");
      return 3;
   }
   if (!verified_policy)
   {
      fprintf(stderr, "FAIL: player window observed but activation policy unavailable\n");
      return 4;
   }
   return 0;
}
