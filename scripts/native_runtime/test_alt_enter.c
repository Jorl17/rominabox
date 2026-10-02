/* Alt+Enter toggles fullscreen on Windows and Linux. Enter alone does not.
 * On a Mac the window goes fullscreen in a different way, so there Return
 * with Alt is Return, nothing toggles, and the game receives every key.
 *
 * We leave input_toggle_fullscreen nul in the exported config, because f is a
 * gameplay key. These cases call the same functions as the runloop.
 */
#include "input/alt_enter_fullscreen.h"

#if defined(__APPLE__)
#define CHORD 0
#else
#define CHORD 1
#endif

#include <stdio.h>
#include <stdlib.h>

static int failures = 0;

static void expect_due(const char *name, int expect)
{
   int got = alt_enter_fullscreen_due();
   if (got == expect)
      return;
   fprintf(stderr, "FAIL %s: fullscreen %s\n",
         name, expect ? "did not fire" : "fired");
   failures++;
}

static void expect_mask(const char *name, int expect)
{
   int got = alt_enter_masks_return();
   if (got == expect)
      return;
   fprintf(stderr, "FAIL %s: Return %s the game\n",
         name, expect ? "reached" : "was kept from");
   failures++;
}

/* Hold Alt, press Enter. The window should go fullscreen. */
static void alt_enter_toggles(void)
{
   alt_enter_reset();
   alt_enter_note(ALT_ENTER_RETURN, 1, ALT_ENTER_ALT);
   expect_due("alt+enter", CHORD);
   expect_mask("alt+enter", CHORD);
}

/* Enter is Start. It must not also change the window. */
static void enter_alone_does_not_toggle(void)
{
   alt_enter_reset();
   alt_enter_note(ALT_ENTER_RETURN, 1, 0);
   expect_due("enter alone", 0);
   expect_mask("enter alone", 0);
}

/* Alt by itself is not the chord. The modifier arrives on Return. */
static void alt_alone_does_not_toggle(void)
{
   alt_enter_reset();
   alt_enter_note(308, 1, 0); /* RETROK_LALT */
   expect_due("alt alone", 0);
   expect_mask("alt alone", 0);
}

/* A held key repeats. The second sample must not toggle back. */
static void repeat_does_not_toggle_again(void)
{
   alt_enter_reset();
   alt_enter_note(ALT_ENTER_RETURN, 1, ALT_ENTER_ALT);
   expect_due("first alt+enter", CHORD);
   alt_enter_note(ALT_ENTER_RETURN, 1, ALT_ENTER_ALT);
   expect_due("repeated alt+enter", 0);
   expect_mask("repeated alt+enter", CHORD);
}

/* Releasing Enter ends the chord. The next press toggles again. */
static void release_arms_the_next_press(void)
{
   alt_enter_reset();
   alt_enter_note(ALT_ENTER_RETURN, 1, ALT_ENTER_ALT);
   expect_due("press", CHORD);
   alt_enter_note(ALT_ENTER_RETURN, 0, ALT_ENTER_ALT);
   expect_due("release", 0);
   expect_mask("release", 0);
   alt_enter_note(ALT_ENTER_RETURN, 1, ALT_ENTER_ALT);
   expect_due("press again", CHORD);
}

/* Down and up both happen between two samples. The press still counts,
 * once, and we pass Return through to the game. */
static void press_between_samples_toggles(void)
{
   alt_enter_reset();
   alt_enter_note(ALT_ENTER_RETURN, 1, ALT_ENTER_ALT);
   alt_enter_note(ALT_ENTER_RETURN, 0, ALT_ENTER_ALT);
   expect_due("alt+enter between samples", CHORD);
   expect_due("the sample after it", 0);
   expect_mask("the sample after it", 0);
}

/* Numpad Enter is the same chord. Shift+Enter is not. */
static void keypad_enter_counts_and_shift_enter_does_not(void)
{
   alt_enter_reset();
   alt_enter_note(ALT_ENTER_KP_RETURN, 1, ALT_ENTER_ALT);
   expect_due("alt+keypad enter", CHORD);
   alt_enter_reset();
   alt_enter_note(ALT_ENTER_RETURN, 1, 0x01); /* RETROKMOD_SHIFT */
   expect_due("shift+enter", 0);
   expect_mask("shift+enter", 0);
}

/* We check this in the menu text entry before using a Return for its form. */
static void the_menu_recognises_the_chord(void)
{
   const struct { unsigned code, modifiers; int chord; const char *name; } cases[] = {
      {ALT_ENTER_RETURN, ALT_ENTER_ALT, CHORD, "alt+return"},
      {ALT_ENTER_KP_RETURN, ALT_ENTER_ALT | 0x01u, CHORD, "shift+alt+keypad enter"},
      {ALT_ENTER_RETURN, 0, 0, "return"},
      {'a', ALT_ENTER_ALT, 0, "alt+a"},
   };
   size_t index;
   for (index = 0; index < sizeof(cases) / sizeof(cases[0]); ++index)
      if (!alt_enter_is_chord(cases[index].code, cases[index].modifiers) != !cases[index].chord)
      {
         fprintf(stderr, "FAIL %s: %s the chord\n", cases[index].name,
               cases[index].chord ? "not taken for" : "taken for");
         failures++;
      }
}

int main(void)
{
   alt_enter_toggles();
   enter_alone_does_not_toggle();
   alt_alone_does_not_toggle();
   repeat_does_not_toggle_again();
   release_arms_the_next_press();
   press_between_samples_toggles();
   keypad_enter_counts_and_shift_enter_does_not();
   the_menu_recognises_the_chord();
   if (failures)
   {
      fprintf(stderr, "%d alt+enter check(s) failed\n", failures);
      return 1;
   }
   printf("alt+enter checks passed\n");
   return 0;
}
