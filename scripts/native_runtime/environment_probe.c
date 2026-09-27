/* Print each named variable as we read it in the player
 * (vendor/retroarch/rominabox_environment.h): `name=[value]`, or
 * `name=<unset>`. Print the value as its UTF-8 bytes. */
#include <stdio.h>
#include <stdlib.h>

#include "rominabox_environment.h"

int main(int argc, char **argv)
{
   int index;
   for (index = 1; index < argc; index++)
   {
      char *value = rib_environment(argv[index]);
      if (value)
         printf("%s=[%s]\n", argv[index], value);
      else
         printf("%s=<unset>\n", argv[index]);
      free(value);
   }
   return 0;
}
