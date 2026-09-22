/* The ON row is the bundled preset path that is running.
 * Compile and run. On a wrong index we print the sentence the tests check.
 */
#include <stdio.h>
#include <stdlib.h>

#include "rmlui_shader_mark.h"

static void expect(const char *name, int got, int want)
{
   if (got == want)
      return;
   fprintf(stderr,
         "the row marked ON is not the shader that is running "
         "(%s: got %d, want %d)\n",
         name, got, want);
   exit(1);
}

int main(void)
{
   const char *rows[] = {
      "",
      "shaders/scanlines/scanlines.glslp",
      "shaders/phosphor/phosphor.glslp",
   };
   const char *scanlines =
         "/Game.app/Contents/Resources/menu-assets/shaders/scanlines/scanlines.glslp";
   const char *phosphor =
         "/Game.app/Contents/Resources/menu-assets/shaders/phosphor/phosphor.glslp";

   expect("scanlines", rib_shader_mark_index(scanlines, rows, 3), 1);
   expect("phosphor", rib_shader_mark_index(phosphor, rows, 3), 2);
   expect("off", rib_shader_mark_index(NULL, rows, 3), 0);
   expect("blank", rib_shader_mark_index("", rows, 3), 0);
   expect("unknown", rib_shader_mark_index("/tmp/other.glslp", rows, 3), -1);
   return 0;
}
