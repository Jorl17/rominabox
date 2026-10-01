#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/main.cpp is rml-preview's entry on macOS and Linux; the player recipe names each platform's"
#endif

#include "../rml_preview.h"

#include <string>
#include <vector>

int main(int argc, char **argv)
{
   return run(std::vector<std::string>(argv, argv + argc));
}
