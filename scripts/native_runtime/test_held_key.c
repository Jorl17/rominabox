/* Hold one key, press another.
 *
 * The menu toggle is Escape. The other key is a direction: hold Right,
 * press Escape, and the menu opens without waiting for Right to be
 * released. These cases call the same function as the runloop, and
 * describe the expected behaviour.
 */
#include "input/held_key_policy.h"

#include <stdio.h>
#include <stdlib.h>

#define ESCAPE 27

static int failures = 0;

static void expect_fire(
      const char *name,
      int level,
      int other,
      unsigned *flushing,
      int expect)
{
   int got = held_key_menu_toggle_fires(ESCAPE, level, other, flushing);
   if (got == expect)
      return;
   fprintf(stderr,
         "FAIL %s: menu toggle %s (got %d, flushing %u, level %d, other %d)\n",
         name,
         expect ? "did not fire" : "fired",
         got, *flushing, level, other);
   failures++;
}

/* Escape down on one sample and up on the next, while Right is held and
 * no flush is in progress. This works in the main hotkey loop and must
 * keep working. */
static void normal_press_while_direction_held(void)
{
   unsigned flushing = 0;
   held_key_reset();
   held_key_note(ESCAPE, 1);
   expect_fire("normal press arms", 1, 1, &flushing, 0);
   held_key_note(ESCAPE, 0);
   expect_fire("normal release toggles", 0, 1, &flushing, 1);
}

/* Opening or closing the menu sets the flush counter to 2. While a
 * direction stays down the counter never reaches 0, and we clear the
 * sample that would toggle the menu. After more samples than the
 * counter, releasing Escape must still toggle. */
static void flush_does_not_stick_while_direction_held(void)
{
   unsigned flushing = 2;
   int i;
   held_key_reset();
   for (i = 0; i < 8; i++)
      expect_fire("flush counts down while Right is held", 0, 1, &flushing, 0);
   if (flushing != 0)
   {
      fprintf(stderr,
            "FAIL flush stuck at %u while Right was held\n", flushing);
      failures++;
   }
   held_key_note(ESCAPE, 1);
   expect_fire("escape press after flush", 1, 1, &flushing, 0);
   held_key_note(ESCAPE, 0);
   expect_fire("escape release after flush toggles", 0, 1, &flushing, 1);
}

/* A flush also sets the "wait until every button is up" latch. Once the
 * counter has reached 0, the direction is still held and the game is
 * moving, but the latch is still set, so we ignore Escape until Right is
 * released. The latch must not outlast the flush. */
static void wait_does_not_outlive_flush(void)
{
   unsigned flushing = 2;
   held_key_reset();
   expect_fire("flush sample while Right is held", 0, 1, &flushing, 0);
   flushing = 0;
   held_key_note(ESCAPE, 1);
   expect_fire("escape press is not swallowed by the wait latch", 1, 1, &flushing, 0);
   held_key_note(ESCAPE, 0);
   expect_fire("escape release is not swallowed by the wait latch", 0, 1, &flushing, 1);
}

/* A down and an up between two samples never appear in the level, but
 * the press still happened. */
static void press_and_release_inside_one_sample(void)
{
   unsigned flushing = 0;
   held_key_reset();
   held_key_note(ESCAPE, 1);
   held_key_note(ESCAPE, 0);
   expect_fire("escape down and up inside one sample toggles", 0, 1, &flushing, 1);
}

int main(void)
{
   normal_press_while_direction_held();
   flush_does_not_stick_while_direction_held();
   wait_does_not_outlive_flush();
   press_and_release_inside_one_sample();
   if (failures)
   {
      fprintf(stderr, "%d held-key check(s) failed\n", failures);
      return 1;
   }
   printf("held-key checks passed\n");
   return 0;
}
