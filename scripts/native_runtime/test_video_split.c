/* How we split the brightness the player chose between the brightness
 * parameter of the chosen shader and our pass. Above 100 % we raise the
 * parameter by its table, up to the top, and multiply the rest with our pass.
 * At or below 100 % we give the parameter its value in the preset again, so
 * that a running shader we raised before does not stay raised.
 */
#include "menu/drivers/rmlui/video_split.h"

#include <stdio.h>
#include <string.h>

static int failures = 0;

static int near(float a, float b)
{
   float difference = a - b;
   return (difference < 0.0f ? -difference : difference) < 0.001f;
}

static void expect(const char *name, struct rib_video_split split,
      const char *parameter, float value, float pass)
{
   const int same_parameter = parameter
         ? split.parameter && !strcmp(split.parameter, parameter)
         : !split.parameter;
   if (same_parameter && (!parameter || near(split.value, value)) && near(split.pass, pass))
      return;
   fprintf(stderr, "FAIL %s: %s at %.3f and %.3f for our pass, where we expect %s at %.3f and %.3f\n",
         name, split.parameter ? split.parameter : "no parameter", split.value, split.pass,
         parameter ? parameter : "no parameter", value, pass);
   failures++;
}

int main(void)
{
   static struct rib_video_control controls[2];
   const char *lottes = "/game/menu/shaders/glsl/crt/crt-lottes.glslp";
   if (!rib_video_control_read(&controls[0], "shaders/glsl/crt/crt-lottes.glslp",
            "brightBoost 1:1 1.5:1.4 2:1.8")
         || controls[0].count != 3 || strcmp(controls[0].parameter, "brightBoost"))
   {
      fprintf(stderr, "FAIL reading a table of three values\n");
      failures++;
   }
   if (rib_video_control_read(&controls[1], "shaders/glsl/crt/zfast-crt.glslp", "BRIGHTBOOST 1:1.25"))
   {
      fprintf(stderr, "FAIL taking a table of one value\n");
      failures++;
   }
   expect("within the table", rib_video_split_brightness(controls, 1, lottes, 1.25f),
         "brightBoost", 1.2f, 1.0f);
   expect("above the table", rib_video_split_brightness(controls, 1, lottes, 3.0f),
         "brightBoost", 1.8f, 1.5f);
   expect("at 100 %", rib_video_split_brightness(controls, 1, lottes, 1.0f),
         "brightBoost", 1.0f, 1.0f);
   expect("below 100 %", rib_video_split_brightness(controls, 1, lottes, 0.85f),
         "brightBoost", 1.0f, 0.85f);
   expect("a shader without a table",
         rib_video_split_brightness(controls, 1, "/game/menu/shaders/glsl/crt/zfast-crt.glslp", 1.4f),
         NULL, 0.0f, 1.4f);
   expect("no shader", rib_video_split_brightness(controls, 1, "", 1.4f), NULL, 0.0f, 1.4f);
   if (failures)
      return 1;
   printf("brightness split checks passed\n");
   return 0;
}
