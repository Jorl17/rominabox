#pragma once

#include <string>
#include <vector>

/* Run rml-preview with its arguments, the program's name first, each one in
 * UTF-8, and return its exit code. We pass them from the entry for each
 * platform, windows/main.cpp or posix/main.cpp. */
int run(const std::vector<std::string>& arguments);
