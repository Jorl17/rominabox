/* The run loop's decision whether the game window counts as focused when
 * the player keeps the game from playing in the background. */
#include <stdio.h>
#include "rominabox_background_pause.h"

static int failures;

static void check(int ok, const char *what)
{
   if (!ok)
   {
      fprintf(stderr, "FAIL background pause: %s\n", what);
      ++failures;
   }
}

int main(void)
{
   check(rib_focused_for_pause(true, true), "a game in front plays");
   check(!rib_focused_for_pause(false, true), "a game that has run pauses in the background");
   check(rib_focused_for_pause(false, false),
         "a game that has not run yet plays on in the background, so its splash and first frame are drawn");
   if (!failures)
      printf("background pause: every case passed\n");
   return failures ? 1 : 0;
}
