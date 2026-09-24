/* External RetroArch keyboard boundary shared by the menu/input probes. */
#include "rmlui/text_host.h"
#include <string>
std::string keyboard_value;
bool keyboard_active;
rib_text_complete complete;
void *complete_context;
extern "C" {
bool rib_host_keyboard_begin(const char *value, rib_text_complete callback, void *context) {
   keyboard_value = value; keyboard_active = true; complete = callback; complete_context = context; return true;
}
void rib_host_keyboard_end() { keyboard_active = false; keyboard_value.clear(); }
bool rib_host_keyboard_active() { return keyboard_active; }
const char *rib_host_keyboard_value() { return keyboard_value.c_str(); }
void rib_host_keyboard_replace(const char *value) { keyboard_value = value; }
const char *rib_host_keyboard_label(unsigned) { return "x"; }
int rib_host_keyboard_focus() { return 0; }
void rib_host_keyboard_choose(unsigned) { keyboard_value += "x"; }
void rib_host_text_focus(bool) {}
}
