/* miniz itself (vendor/miniz), with the options in zip_library.h. We compile
 * it as the library's own code, without our warnings. */
#include "zip_library.h"
#include "miniz.c"
