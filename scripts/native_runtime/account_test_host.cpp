/* The RetroArch host boundary for the RmlUi account tests. The stand-in
 * for the achievements service is in achievements_fake.cpp. */
#include "account_test_host.hpp"
#include "rmlui/text_host.h"
int quits;
bool overlay_frames;
bool menu_open = true;
int64_t host_time_us = 1000000;
extern "C" {
int64_t rib_host_time_us() { return host_time_us; }
void rib_host_overlay_frames(bool on) { overlay_frames = on; }
bool rib_host_menu_open() { return menu_open; }
void rib_host_quit() { ++quits; }
}
