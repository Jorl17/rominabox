/* Headless regression checks against the actual RmlUi bridge and domain helpers.
 * We compile rmlui_bridge.cpp with a dummy renderer and create no window. */

#include "rmlui_bridge.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>
/* rmlui.c is not linked here because it depends on the whole of RetroArch,
 * so we stub the control list that it normally supplies. With the stub a test
 * can declare more than sixteen controls, and the bridge must address all of
 * them. A PlayStation DualShock declares twenty-four. */
static const char *stub_control_ids[] = {
   "up", "down", "left", "right", "a", "b", "x", "y",
   "l", "r", "l2", "r2", "l3", "r3", "start", "select",
   "l_x_plus", "l_x_minus", "l_y_plus", "l_y_minus",
   "r_x_plus", "r_x_minus", "r_y_plus", "r_y_minus"
};
static const int stub_control_count =
   (int)(sizeof(stub_control_ids) / sizeof(stub_control_ids[0]));

/* Two controllers, so there is a choice in the picker, as on the Mega Drive. */
static const char *stub_device_ids[] = {"megadrive", "megadrive6"};
static const char *stub_device_names[] = {"Mega Drive", "Mega Drive six-button"};

extern "C" int rib_rmlui_device_count(void) { return 2; }
extern "C" const char *rib_rmlui_device_id(int index)
{
   return (index >= 0 && index < 2) ? stub_device_ids[index] : nullptr;
}
extern "C" const char *rib_rmlui_device_name(int index)
{
   return (index >= 0 && index < 2) ? stub_device_names[index] : nullptr;
}

extern "C" int rib_rmlui_control_capacity(void) { return stub_control_count; }
extern "C" const char *rib_rmlui_control_id(int index)
{
   if (index < 0 || index >= stub_control_count)
      return nullptr;
   return stub_control_ids[index];
}
extern "C" const char *rib_rmlui_control_group(int) { return nullptr; }

/* The cue that we requested from the sound pack for a move. Empty until a
 * step changes a level. The player has the production function, and this
 * one only records the call. */
static std::string move_sound_log;
extern "C" void rib_rmlui_play_move_sound(int direction)
{
   if (!move_sound_log.empty())
      move_sound_log.push_back(' ');
   move_sound_log += direction > 0 ? "up" : "down";
}

extern "C" unsigned rib_rmlui_test_texture_loads();
extern "C" const char *rib_rmlui_test_property(const char *, const char *);
extern "C" bool rib_rmlui_test_box(const char *, int *, int *, int *, int *);
extern "C" bool rib_rmlui_test_row_glyphs_overlap(const char *);
extern "C" int rib_rmlui_test_class_count(const char *);
extern "C" const char *rib_rmlui_test_class_id(const char *, int);

extern "C" void rib_rmlui_test_advance(double);
extern "C" const char *rib_rmlui_test_text(const char *);
extern "C" bool rib_rmlui_test_has_class(const char *, const char *);
extern "C" float rib_rmlui_test_picture_aspect();
static int failures = 0;

#define CHECK(cond, msg) \
   do { \
      if (!(cond)) { \
         std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, msg); \
         ++failures; \
      } \
   } while (0)

static void click_id(const char *id)
{
   int x = 0;
   int y = 0;
   CHECK(rib_rmlui_element_center(id, &x, &y), "element has a hit centre");
   rib_rmlui_pointer_move(x, y);
   rib_rmlui_pointer_button(true);
   rib_rmlui_pointer_button(false);
}

static void move_to_id(const char *id)
{
   int x = 0;
   int y = 0;
   CHECK(rib_rmlui_element_center(id, &x, &y), "element has a hover centre");
   rib_rmlui_pointer_move(x, y);
}

// Press on one element and release somewhere else. People do this: they put
// the button down, change their mind, slide off and let go. Nothing should
// happen.
static void press_then_release_at(const char *id, int x, int y)
{
   int from_x = 0;
   int from_y = 0;
   CHECK(rib_rmlui_element_center(id, &from_x, &from_y), "element has a hit centre");
   rib_rmlui_pointer_move(from_x, from_y);
   rib_rmlui_pointer_button(true);
   rib_rmlui_pointer_move(x, y);
   rib_rmlui_pointer_button(false);
}

static void drain_actions(void)
{
   while (rib_rmlui_take_action() != RIB_RMLUI_ACTION_NONE)
      ;
}

/* Draw for this long. RmlUi advances an animation by at most a tenth of a
 * second per update, so a transition finishes only if we draw frames while
 * the clock moves, as at sixty frames a second in a running game. */
static void settle(double seconds)
{
   for (double at = 0; at < seconds; at += 0.05)
   {
      rib_rmlui_test_advance(0.05);
      rib_rmlui_render(960, 600);
   }
}

struct Box { int x, y, w, h; bool ok; };

static Box box_of(const char *id)
{
   Box box{};
   box.ok = rib_rmlui_test_box(id, &box.x, &box.y, &box.w, &box.h);
   return box;
}

static bool boxes_overlap(const Box &a, const Box &b)
{
   return a.x < b.x + b.w && b.x < a.x + a.w
         && a.y < b.y + b.h && b.y < a.y + a.h;
}

/* Positive when the boxes are apart on x. Zero is touching, which still
 * hides the thumb under the arrow. */
static int horizontal_gap(const Box &a, const Box &b)
{
   if (a.x + a.w <= b.x)
      return b.x - (a.x + a.w);
   if (b.x + b.w <= a.x)
      return a.x - (b.x + b.w);
   const int overlap = std::min(a.x + a.w, b.x + b.w) - std::max(a.x, b.x);
   return -overlap;
}

/* Flush with the edge is the cut corner: in popup-a-off-right-edge the list
 * border is on the window's last pixel. Touching the edge counts as off. */
static bool inside_screen(const Box &box, const Box &screen)
{
   return box.x >= screen.x && box.y >= screen.y
         && box.x + box.w < screen.x + screen.w
         && box.y + box.h < screen.y + screen.h;
}

static void fill_bind_rows(void)
{
   const int rows = rib_rmlui_rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const char *id = rib_rmlui_row_in("control-binds", index);
      if (!id || !*id)
         break;
      if (index < 2)
      {
         /* The long line is what a fixed width would size every list for: a
          * list of "UP / UP / KEY" would be as wide as "LEFT STICK UP / BUTTON 12". */
         rib_rmlui_set_row_text(id,
               index == 0 ? "LEFT STICK UP" : "UP",
               index == 0 ? "BUTTON 12" : "HAT #0 UP",
               index == 0 ? "AXIS" : "PAD");
         rib_rmlui_set_shown(id, true);
      }
      else
         rib_rmlui_set_shown(id, false);
   }
   rib_rmlui_retarget_pages("control-binds");
}

static int anchors(const char *class_name, std::vector<std::string> &out)
{
   const int count = rib_rmlui_test_class_count(class_name);
   for (int index = 0; index < count; ++index)
   {
      const char *id = rib_rmlui_test_class_id(class_name, index);
      if (id && *id)
         out.emplace_back(id);
   }
   return count;
}

/* How many labels this anchor may still cover. -1 means none. Because the
 * list may cover the drawing, every control, including the right stick,
 * has a position that covers no label. */
static int least_allowed(const char *profile, const char *anchor)
{
   (void)profile;
   (void)anchor;
   return -1;
}

static std::vector<std::string> least_cover_seen;

struct BindLine
{
   std::string anchor;
   std::string title;
   std::string detail;
   std::string kind;
};

static std::vector<BindLine> bind_lines;

/* One line per bind in the list: the label and key of the control, then the
 * short pad word (Hat, Axis, Button), not "LEFT STICK UP / BUTTON 12". */
static void load_bind_lines(const std::filesystem::path &scene)
{
   bind_lines.clear();
   std::filesystem::path lines = scene;
   lines.replace_extension(".lines");
   std::ifstream in(lines);
   std::string row;
   while (std::getline(in, row))
   {
      if (row.empty())
         continue;
      BindLine line;
      std::stringstream fields(row);
      if (!std::getline(fields, line.anchor, '\t'))
         continue;
      std::getline(fields, line.title, '\t');
      std::getline(fields, line.detail, '\t');
      std::getline(fields, line.kind, '\t');
      if (!line.anchor.empty())
         bind_lines.push_back(line);
   }
}

static void fill_anchor_rows(const char *anchor)
{
   std::vector<const BindLine*> matched;
   for (const BindLine &line : bind_lines)
      if (line.anchor == anchor)
         matched.push_back(&line);
   if (matched.size() < 2)
      return;
   const int rows = rib_rmlui_rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const char *id = rib_rmlui_row_in("control-binds", index);
      if (!id || !*id)
         break;
      if (index < (int)matched.size())
      {
         rib_rmlui_set_row_text(id, matched[index]->title.c_str(),
               matched[index]->detail.c_str(), matched[index]->kind.c_str());
         rib_rmlui_set_shown(id, true);
      }
      else
         rib_rmlui_set_shown(id, false);
   }
   rib_rmlui_retarget_pages("control-binds");
}

/* Inside the outline, not on it. Flush with the border box means that the
 * row or the pager covers the border, which causes the missing right edge
 * and the pager on the bottom line. */
static bool inside_box(const Box &child, const Box &parent)
{
   return child.x > parent.x && child.y > parent.y
         && child.x + child.w < parent.x + parent.w
         && child.y + child.h < parent.y + parent.h;
}

static const char *const list_part_ids[] = {
   "binds-pager", "binds-prev", "binds-next", "binds-page-count", nullptr
};

/* The buttons of the screen, the status line and the footer. The list may
 * never cover these, even when it covers least there. The pager buttons share
 * the menu-action class but belong to the list, so they are not here. */
/* We read this from the controls document instead of guessing. While that
 * screen is up, the readable and pressable pieces are the heading, the
 * controller picker (its label and button, not the option list, which is
 * display:none), Back, Reset, Cancel when shown, the status line and the
 * footer. The drawing, the leader lines and the hit rings are not in it. */
static const char *const chrome_ids[] = {
   "heading", "controls-device-label", "controls-device-current",
   "controls-back", "controls-reset", "controls-status", "footer", nullptr
};

static bool list_part(const std::string &id)
{
   return id.rfind("binds-", 0) == 0;
}

static int stacking_rank(const char *value)
{
   if (!value || !*value || std::strcmp(value, "auto") == 0)
      return 0;
   return std::atoi(value);
}

/* We must paint the focused callout, and the one under the pointer, above
 * the next control. Otherwise the box of L2 covers the bottom edge of L1. */
static void check_drawn_above(const char *design)
{
   std::vector<std::string> callouts;
   anchors("control-callout", callouts);
   char message[512];
   if (callouts.size() < 2)
   {
      std::snprintf(message, sizeof(message),
            "%s: need two callouts to see which one paints on top", design);
      CHECK(false, message);
      return;
   }
   const std::string &focused = callouts[0];
   const std::string &neighbour = callouts[1];
   const std::string focused_id = focused.substr(std::strlen("control-"));
   const std::string neighbour_id = neighbour.substr(std::strlen("control-"));
   rib_rmlui_set_control_state(focused_id.c_str(), "A", "a", true, false);
   rib_rmlui_set_control_state(neighbour_id.c_str(), "B", "b", false, false);
   /* test_property uses one buffer, so we copy the value before the next
    * read. */
   const std::string focused_z = rib_rmlui_test_property(focused.c_str(), "z-index");
   const std::string neighbour_z = rib_rmlui_test_property(neighbour.c_str(), "z-index");
   std::snprintf(message, sizeof(message),
         "%s: focused %s z-index is '%s' and %s is '%s'; the focused control is under its neighbour",
         design, focused.c_str(), focused_z.c_str(),
         neighbour.c_str(), neighbour_z.c_str());
   CHECK(stacking_rank(focused_z.c_str()) > stacking_rank(neighbour_z.c_str()), message);

   move_to_id(neighbour.c_str());
   const std::string hover_z = rib_rmlui_test_property(neighbour.c_str(), "z-index");
   std::snprintf(message, sizeof(message),
         "%s: hovered %s z-index is '%s'; the control under the pointer is under its neighbour",
         design, neighbour.c_str(), hover_z.c_str());
   CHECK(stacking_rank(hover_z.c_str()) > 0, message);

   if (rib_rmlui_has_element("control-group-l_stick"))
   {
      rib_rmlui_focus_group("l_stick");
      const std::string group_z = rib_rmlui_test_property(
            "control-group-l_stick", "z-index");
      std::snprintf(message, sizeof(message),
            "%s: focused stick group z-index is '%s'",
            design, group_z.c_str());
      CHECK(stacking_rank(group_z.c_str()) > 0, message);
      rib_rmlui_focus_group(nullptr);
   }

   rib_rmlui_set_control_state(focused_id.c_str(), "A", "a", false, false);
   rib_rmlui_pointer_move(1, 1);
}

static void collect_painted(const Box &list, std::vector<Box> &painted)
{
   painted.clear();
   if (list.ok)
      painted.push_back(list);
   const int row_count = rib_rmlui_rows_in("control-binds");
   for (int index = 0; index < row_count; ++index)
   {
      const char *row_id = rib_rmlui_row_in("control-binds", index);
      const Box row = row_id ? box_of(row_id) : Box{};
      if (row.ok)
         painted.push_back(row);
   }
   for (int index = 0; list_part_ids[index]; ++index)
   {
      const Box part = box_of(list_part_ids[index]);
      if (part.ok)
         painted.push_back(part);
   }
}

/* Every row, its separator (the border of the row) and every pager button
 * must be inside the list border. Being inside the window is not enough. */
static int check_parts_inside_list(const char *design, const char *profile,
      const char *anchor, int window_w, int window_h, const Box &list)
{
   int missed = 0;
   char message[512];
   const int row_count = rib_rmlui_rows_in("control-binds");
   for (int index = 0; index < row_count; ++index)
   {
      const char *row_id = rib_rmlui_row_in("control-binds", index);
      const Box row = row_id ? box_of(row_id) : Box{};
      if (!row.ok)
         continue;
      if (inside_box(row, list))
         continue;
      std::snprintf(message, sizeof(message),
            "%s/%s %s at %dx%d: %s (and its separator) %d,%d %dx%d paints outside the list border %d,%d %dx%d",
            design, profile, anchor, window_w, window_h, row_id,
            row.x, row.y, row.w, row.h, list.x, list.y, list.w, list.h);
      CHECK(false, message);
      ++missed;
   }
   for (int index = 0; list_part_ids[index]; ++index)
   {
      const Box part = box_of(list_part_ids[index]);
      if (!part.ok || inside_box(part, list))
         continue;
      std::snprintf(message, sizeof(message),
            "%s/%s %s at %dx%d: %s %d,%d %dx%d hangs outside the list border %d,%d %dx%d",
            design, profile, anchor, window_w, window_h, list_part_ids[index],
            part.x, part.y, part.w, part.h, list.x, list.y, list.w, list.h);
      CHECK(false, message);
      ++missed;
   }
   return missed;
}

static int check_one_list(const char *design, const char *profile,
      const char *anchor, int width, int window_w, int window_h)
{
   char message[512];
   fill_anchor_rows(anchor);
   rib_rmlui_place_list("control-binds", anchor, width);
   const Box screen = box_of("screen");
   const Box list = box_of("control-binds");
   if (!screen.ok || !list.ok)
   {
      std::snprintf(message, sizeof(message),
            "%s/%s %s at %dx%d: the list or the screen has no box",
            design, profile, anchor, window_w, window_h);
      CHECK(false, message);
      return 1;
   }
   int missed = 0;
   {
      static bool stand_in_noted = false;
      const char *first = rib_rmlui_row_in("control-binds", 0);
      const std::string title_id = first ? std::string(first) + "-title" : "";
      const char *title = first ? rib_rmlui_test_text(title_id.c_str()) : "";
      if (!stand_in_noted && title && std::strcmp(title, "LEFT STICK UP") == 0)
      {
         stand_in_noted = true;
         CHECK(false,
               "placement still fills the list with the stand-in LEFT STICK UP / BUTTON 12, not the control's own bind");
         ++missed;
      }
   }
   missed += check_parts_inside_list(design, profile, anchor, window_w, window_h, list);
   if (!inside_screen(list, screen))
   {
      std::snprintf(message, sizeof(message),
            "%s/%s %s at %dx%d: list %d,%d %dx%d leaves the screen %d,%d %dx%d",
            design, profile, anchor, window_w, window_h,
            list.x, list.y, list.w, list.h,
            screen.x, screen.y, screen.w, screen.h);
      CHECK(false, message);
      ++missed;
   }
   /* A row is width 100% plus its border, so it extends past the list box
    * measured for the clamp. That is the strip cut off in the corner. */
   const int row_count = rib_rmlui_rows_in("control-binds");
   for (int index = 0; index < row_count; ++index)
   {
      const char *row_id = rib_rmlui_row_in("control-binds", index);
      const Box row = row_id ? box_of(row_id) : Box{};
      if (!row.ok || inside_screen(row, screen))
         continue;
      std::snprintf(message, sizeof(message),
            "%s/%s %s at %dx%d: %s %d,%d %dx%d leaves the screen %d,%d %dx%d",
            design, profile, anchor, window_w, window_h, row_id,
            row.x, row.y, row.w, row.h,
            screen.x, screen.y, screen.w, screen.h);
      CHECK(false, message);
      ++missed;
   }
   std::vector<std::string> labels;
   anchors("control-callout", labels);
   anchors("control-group", labels);
   std::vector<std::string> covered;
   std::vector<Box> painted;
   painted.push_back(list);
   for (int index = 0; index < row_count; ++index)
   {
      const char *row_id = rib_rmlui_row_in("control-binds", index);
      const Box row = row_id ? box_of(row_id) : Box{};
      if (row.ok)
         painted.push_back(row);
   }
   for (int index = 0; list_part_ids[index]; ++index)
   {
      const Box part = box_of(list_part_ids[index]);
      if (part.ok)
         painted.push_back(part);
   }
   std::vector<std::string> chrome;
   for (int index = 0; chrome_ids[index]; ++index)
      chrome.emplace_back(chrome_ids[index]);
   {
      std::vector<std::string> actions;
      anchors("menu-action", actions);
      for (const std::string &action : actions)
         if (!list_part(action)
               && std::find(chrome.begin(), chrome.end(), action) == chrome.end())
            chrome.push_back(action);
   }
   for (const std::string &id : chrome)
   {
      const Box other = box_of(id.c_str());
      if (!other.ok)
         continue;
      bool hit = false;
      for (const Box &part : painted)
         hit = hit || boxes_overlap(part, other);
      if (!hit)
         continue;
      std::snprintf(message, sizeof(message),
            "%s/%s %s at %dx%d: list covers %s",
            design, profile, anchor, window_w, window_h, id.c_str());
      CHECK(false, message);
      ++missed;
   }
   for (const std::string &label : labels)
   {
      /* The player is reading the control of the list. Its box counts like a
       * button, and the list may never cover it. */
      if (label == anchor)
      {
         const Box own = box_of(label.c_str());
         bool hit = own.ok;
         if (hit)
         {
            hit = false;
            for (const Box &part : painted)
               hit = hit || boxes_overlap(part, own);
         }
         if (hit)
         {
            std::snprintf(message, sizeof(message),
                  "%s/%s %s at %dx%d: list covers its own control",
                  design, profile, anchor, window_w, window_h);
            CHECK(false, message);
            ++missed;
         }
         continue;
      }
      const Box other = box_of(label.c_str());
      if (!other.ok)
         continue;
      bool hit = false;
      for (const Box &part : painted)
         hit = hit || boxes_overlap(part, other);
      if (hit)
         covered.push_back(label);
   }
   const int least = least_allowed(profile, anchor);
   if (!covered.empty() && least >= 0 && (int)covered.size() <= least)
   {
      std::string key = std::string(design) + "/" + profile + " " + anchor;
      for (const std::string &label : covered)
         key += " covers " + label;
      if (std::find(least_cover_seen.begin(), least_cover_seen.end(), key)
            == least_cover_seen.end())
         least_cover_seen.push_back(key);
      return missed;
   }
   for (const std::string &label : covered)
   {
      std::snprintf(message, sizeof(message),
            "%s/%s %s at %dx%d: list covers %s",
            design, profile, anchor, window_w, window_h, label.c_str());
      CHECK(false, message);
      ++missed;
   }
   return missed;
}

static void check_glyphs(const char *design, const char *which)
{
   const int rows = rib_rmlui_rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const char *id = rib_rmlui_row_in("control-binds", index);
      if (!id || !*id || !rib_rmlui_test_row_glyphs_overlap(id))
         continue;
      char message[256];
      std::snprintf(message, sizeof(message),
            "%s %s row %s: the label is drawn on top of the binding",
            design, which, id);
      CHECK(false, message);
   }
}

/* A fixed width would make "UP / UP / KEY" as wide as "LEFT STICK UP / BUTTON 12".
 * Two short rows and a hidden pager have nothing that needs the ceiling. */
static void check_short_list(const char *design, int declared)
{
   /* In the placement sweep we use the words of each control. Here we use the
    * long stand-in, the text that a fixed width would have to fit. */
   fill_bind_rows();
   check_glyphs(design, "long");
   const int rows = rib_rmlui_rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const char *id = rib_rmlui_row_in("control-binds", index);
      if (!id || !*id)
         break;
      if (index < 2)
         rib_rmlui_set_row_text(id, "A", "BUTTON 2", "PAD");
      else
         rib_rmlui_set_shown(id, false);
   }
   rib_rmlui_retarget_pages("control-binds");
   rib_rmlui_render(960, 600);
   rib_rmlui_place_list("control-binds", "control-up", declared);
   const char *prop = rib_rmlui_test_property("control-binds", "width");
   int parsed = 0;
   if (prop)
      std::sscanf(prop, "%d", &parsed);
   char message[256];
   std::snprintf(message, sizeof(message),
         "%s short list width is %s; two short words should be under 200dp (declared %d)",
         design, prop ? prop : "(none)", declared);
   CHECK(parsed > 0 && parsed < 200 && parsed <= declared, message);
   check_glyphs(design, "short");
}

static void check_volume_ends(const char *design, int window_w, int window_h)
{
   char message[384];
   const struct { float fraction; const char *end; const char *arrow; } ends[] = {
      {0.f, "quiet", RIB_VOLUME_DOWN_ID},
      {1.f, "normal", RIB_VOLUME_UP_ID},
   };
   for (const auto &end : ends)
   {
      rib_rmlui_set_slider(RIB_VOLUME_SLIDER_ID, end.fraction, nullptr);
      const Box thumb = box_of("volume-level-thumb");
      const Box arrow = box_of(end.arrow);
      const int gap = (thumb.ok && arrow.ok) ? horizontal_gap(thumb, arrow) : -1;
      const bool covered = thumb.ok && arrow.ok && boxes_overlap(thumb, arrow);
      std::snprintf(message, sizeof(message),
            "%s volume at %s (%dx%d): thumb %d,%d %dx%d arrow %s %d,%d %dx%d gap %d",
            design, end.end, window_w, window_h,
            thumb.x, thumb.y, thumb.w, thumb.h, end.arrow,
            arrow.x, arrow.y, arrow.w, arrow.h, gap);
      CHECK(thumb.ok && arrow.ok && !covered && gap >= 1, message);
   }
}

/* The right-stick picture: five rows and a pager, tall enough to reach the
 * status line, the footer and BACK. Two short stand-in rows are not that
 * list, so we fill the pages that the player shows for a stick. */
static void fill_stick_pages(void)
{
   const int rows = rib_rmlui_rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const char *id = rib_rmlui_row_in("control-binds", index);
      if (!id || !*id)
         break;
      if (index < 10)
      {
         rib_rmlui_set_row_text(id, "Right stick up", "Axis -0", "AXIS");
         rib_rmlui_set_shown(id, true);
      }
      else
         rib_rmlui_set_shown(id, false);
   }
   rib_rmlui_retarget_pages("control-binds");
}

static int check_rstick_picture(const char *design, const char *profile, int width)
{
   if (std::strcmp(profile, "ps1") != 0 && std::strcmp(profile, "ps1-analog") != 0)
      return 0;
   if (!rib_rmlui_has_element("control-group-r_stick"))
   {
      char message[256];
      std::snprintf(message, sizeof(message),
            "%s/%s: no right-stick group to place the pager list on",
            design, profile);
      CHECK(false, message);
      return 1;
   }
   fill_stick_pages();
   const int sizes[][2] = {{960, 600}, {1440, 900}, {1920, 1200}};
   int missed = 0;
   for (const auto &size : sizes)
   {
      rib_rmlui_render(size[0], size[1]);
      missed += check_one_list(design, profile, "control-group-r_stick", width,
            size[0], size[1]);
   }
   fill_bind_rows();
   return missed;
}

/* Every drawn pad and the RetroPad grid, in this design.
 *
 * A list must not hide the labels of its neighbours or run off the window.
 * The list for UP leaves LEFT visible, and the list for A stays inside the
 * window once the clamp moves its left edge back. */
static int check_placement(const char *assets, const char *scenes,
      const char *design, int width)
{
   if (!rib_rmlui_init(assets, 960, 600))
   {
      std::fprintf(stderr, "FAIL could not init RmlUi from %s\n", assets);
      return 1;
   }
   rib_rmlui_clear_screens();
   rib_rmlui_declare_screen("pause", "pause-panel", "GAME PAUSED",
         "ESC  CONTINUE", "options");
   rib_rmlui_declare_screen("controls", "controls-panel", "CONTROLS",
         "ESC  BACK", "controls");
   rib_rmlui_declare_screen("options", "options-panel", "OPTIONS",
         "ESC  BACK", "options");

   /* 1920x1200 is a 960x600 window on a 2x display, the size at which the
    * right border of the list is on the last pixel. */
   const int sizes[][2] = {{960, 600}, {1440, 900}, {1920, 1200}};
   rib_rmlui_show_screen("options");
   for (const auto &size : sizes)
   {
      rib_rmlui_render(size[0], size[1]);
      check_volume_ends(design, size[0], size[1]);
   }

   rib_rmlui_show_screen("controls");
   fill_bind_rows();
   int scenes_seen = 0;
   for (const auto &entry : std::filesystem::directory_iterator(scenes))
   {
      if (entry.path().extension() != ".rml")
         continue;
      const std::string profile = entry.path().stem().string();
      std::ifstream in(entry.path());
      std::stringstream buffer;
      buffer << in.rdbuf();
      const std::string markup = buffer.str();
      if (!rib_rmlui_set_scene(markup.c_str()))
      {
         char message[256];
         std::snprintf(message, sizeof(message),
               "%s/%s: the scene did not load", design, profile.c_str());
         CHECK(false, message);
         continue;
      }
      ++scenes_seen;
      load_bind_lines(entry.path());
      std::vector<std::string> labels;
      /* Lay out once so the labels exist before we count them. A scene
       * swapped in while its panel is showing still has to be formatted. */
      rib_rmlui_render(960, 600);
      anchors("control-callout", labels);
      anchors("control-group", labels);
      if (labels.empty())
      {
         char message[256];
         std::snprintf(message, sizeof(message),
               "%s/%s: no control labels", design, profile.c_str());
         CHECK(false, message);
         continue;
      }
      if (scenes_seen == 1)
         check_drawn_above(design);
      for (const auto &size : sizes)
      {
         rib_rmlui_render(size[0], size[1]);
         for (const std::string &anchor : labels)
            check_one_list(design, profile.c_str(), anchor.c_str(), width,
                  size[0], size[1]);
      }
      check_rstick_picture(design, profile.c_str(), width);
   }
   {
      char message[128];
      std::snprintf(message, sizeof(message),
            "%s: no controller scenes were staged", design);
      CHECK(scenes_seen > 0, message);
   }
   if (!least_cover_seen.empty())
   {
      std::fprintf(stderr, "least cover (no side misses every label):");
      for (const std::string &name : least_cover_seen)
         std::fprintf(stderr, " %s", name.c_str());
      std::fprintf(stderr, "\n");
   }
   check_short_list(design, width);
   rib_rmlui_shutdown();
   if (failures)
   {
      std::fprintf(stderr, "%d check(s) failed\n", failures);
      return 1;
   }
   return 0;
}

int main(int argc, char **argv)
{
   const char *assets = argc > 1 ? argv[1] : nullptr;
   if (!assets || !*assets)
   {
      std::fprintf(stderr, "usage: test_rmlui_interaction ASSET_DIR\n");
      return 2;
   }
   if (argc > 2 && std::strcmp(argv[2], "placement") == 0)
   {
      if (argc < 6)
      {
         std::fprintf(stderr,
               "usage: test_rmlui_interaction ASSET_DIR placement SCENES DESIGN WIDTH\n");
         return 2;
      }
      return check_placement(assets, argv[3], argv[4], std::atoi(argv[5]));
   }

   CHECK(rib_rmlui_map_menu_toggle(false, true) ==
            RIB_RMLUI_ACTION_CONTROLS_CANCEL,
         "toggle cancels capture first");
   CHECK(rib_rmlui_map_menu_toggle(true, false) ==
            RIB_RMLUI_ACTION_CONTROLS_BACK,
         "toggle leaves Controls next");
   CHECK(rib_rmlui_map_menu_toggle(false, false) ==
            RIB_RMLUI_ACTION_RESUME,
         "toggle resumes from the main screen");
   CHECK(rib_rmlui_toggle_stays_in_menu(true, false),
         "Controls keeps the menu open");
   CHECK(!rib_rmlui_ok_includes_pointer_select(true),
         "RmlUi OK does not consume the pointer select bit");
   CHECK(!rib_rmlui_load_is_actionable(false),
         "empty Load is not actionable");
   CHECK(!rib_rmlui_state_task_matches(false, true, "/s", 1, "/s", 1, true),
         "no pending operation does not match");
   CHECK(!rib_rmlui_state_task_matches(true, true, "/s", 1, "/s", 1, false),
         "a load result does not resolve a save");
   CHECK(!rib_rmlui_state_task_matches(true, true, "/s1", 1, "/s2", 1, true),
         "another path is ignored");
   CHECK(!rib_rmlui_state_task_matches(true, true, "/s", 1, "/s", 2, true),
         "another slot is ignored");
   CHECK(rib_rmlui_state_task_matches(true, false, "/s", 3, "/s", 3, false),
         "exact load path and slot match");

   if (!rib_rmlui_init(assets, 960, 600))
   {
      std::fprintf(stderr, "FAIL could not init RmlUi from %s\n", assets);
      return 1;
   }
   /* The screen button on the pause row is Options. `controls` is inside
    * that panel, so this click cannot reach the built-in handler for
    * `controls`. */
   rib_rmlui_declare_screen("options", "options-panel", "OPTIONS",
         "ESC  BACK", "options");

   rib_rmlui_set_status("SAVED");
   rib_rmlui_set_controls_status("DEFAULTS RESTORED");
   rib_rmlui_test_advance(4);
   rib_rmlui_render(960, 600);
   CHECK(std::string(rib_rmlui_test_text("status")) == "SAVED", "status remains briefly");
   rib_rmlui_test_advance(2);
   rib_rmlui_render(960, 600);
   CHECK(std::string(rib_rmlui_test_text("status")).empty(), "main status expires");
   CHECK(std::string(rib_rmlui_test_text("controls-status")).empty(), "controls status expires");
   for (float aspect : {10.0f/9, 4.0f/3, 16.0f/9}) {
      rib_rmlui_set_game_aspect(aspect);
      CHECK(std::abs(rib_rmlui_test_picture_aspect() - aspect) < 0.02f, "well follows live core aspect");
   }
   rib_rmlui_set_game_aspect(4.0f/3);
   click_id("save");
   click_id("options");
   const int first = rib_rmlui_take_action();
   const int second = rib_rmlui_take_action();
   CHECK(first == RIB_RMLUI_ACTION_SAVE,
         "mailbox preserves the first click");
   // Changing screen has no separate action. We pass the requested screen
   // next to one shared action, so declaring a screen never adds to the
   // enum. This test is mainly about the order, and it also checks that the
   // id arrived.
   CHECK(second == RIB_RMLUI_ACTION_SHOW_SCREEN,
         "mailbox preserves the following click");
   CHECK(std::string(rib_rmlui_requested_screen()) == "options",
         "the screen asked for travels with the action");
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
         "mailbox is empty after both intents");

   move_to_id("resume");
   CHECK(rib_rmlui_hovered_action() == RIB_RMLUI_ACTION_RESUME,
         "pointer hover tracks Resume");
   rib_rmlui_pointer_move(8, 8);
   CHECK(rib_rmlui_hovered_action() == RIB_RMLUI_ACTION_NONE,
         "pointer leave clears hover instead of sticking");

   rib_rmlui_set_focused(RIB_RMLUI_ACTION_QUIT);
   rib_rmlui_set_selected_slot(4);
   move_to_id("resume");
   CHECK(rib_rmlui_hovered_action() == RIB_RMLUI_ACTION_RESUME,
         "hover is independent of keyboard focus");

   rib_rmlui_set_selected_slot(4);
   rib_rmlui_set_focused(RIB_RMLUI_ACTION_RESUME);
   rib_rmlui_pointer_move(1, 1);
   const std::string selected_border = rib_rmlui_test_property("slot-4", "border-top-color");
   move_to_id("slot-4");
   rib_rmlui_set_focused(RIB_RMLUI_ACTION_SELECT_SLOT_1 + 3);
   CHECK(selected_border == rib_rmlui_test_property("slot-4", "border-top-color"),
         "selected slot keeps its border across hover and keyboard focus");
   rib_rmlui_pointer_button(true);
   CHECK(selected_border != rib_rmlui_test_property("slot-4", "border-top-color"),
         "slot has pressed feedback while held");
   rib_rmlui_pointer_move(1, 1);
   rib_rmlui_pointer_button(false);

   rib_rmlui_set_slot_state(1, false, nullptr);
   CHECK(rib_rmlui_element_disabled("load"),
         "empty Load is disabled");
   rib_rmlui_clear_intents();
   click_id("load");
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
         "disabled Load does not enqueue an action");

   rib_rmlui_show_controls(true);
   int control_x = 0, control_y = 0;
   if (rib_rmlui_element_center("control-up", &control_x, &control_y)) {
      rib_rmlui_set_control_state("up", "Up", "up", true, true);
      const std::string animation = rib_rmlui_test_property("control-up", "animation");
      CHECK(animation.find("capture-pulse") != std::string::npos, "capture animates the control itself");
      rib_rmlui_render(960, 600);
      const std::string border = rib_rmlui_test_property("control-up", "border-top-color");
      rib_rmlui_test_advance(0.3);
      rib_rmlui_render(960, 600);
      CHECK(border != rib_rmlui_test_property("control-up", "border-top-color"), "capture border changes over time");
      rib_rmlui_set_control_state("up", "Up", "up", true, false);
      CHECK(std::string(rib_rmlui_test_property("control-up", "animation")).find("capture-pulse") == std::string::npos, "capture cue stops when capture ends");
   }
   rib_rmlui_set_controls_action_focus(false, false, true);
   int cancel_x = 0;
   int cancel_y = 0;
   CHECK(rib_rmlui_element_center("controls-cancel", &cancel_x, &cancel_y),
         "Cancel has a hit centre while capture is visible");
   rib_rmlui_clear_intents();
   rib_rmlui_pointer_move(cancel_x, cancel_y);
   rib_rmlui_pointer_button(true);
   rib_rmlui_pointer_button(false);
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_CONTROLS_CANCEL,
         "Cancel is consumed by RmlUi before any binder poll");

   for (const char *id : {"controls-cancel", "controls-reset", "controls-back"})
   {
      move_to_id(id);
      const std::string hovered = rib_rmlui_test_property(id, "border-top-color");
      rib_rmlui_pointer_button(true);
      CHECK(hovered != rib_rmlui_test_property(id, "border-top-color"),
            "press is visible while pointer remains over a controls button");
      rib_rmlui_pointer_move(1, 1);
      rib_rmlui_pointer_button(false);
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
            "dragging out and releasing does not activate a controls button");
   }

   rib_rmlui_show_controls(false);
   rib_rmlui_clear_intents();
   rib_rmlui_pointer_button(true);
   rib_rmlui_pointer_leave();
   click_id("quit");
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_QUIT,
         "pointer down/up stay in sync after leave");

   rib_rmlui_clear_intents();
   rib_rmlui_show_controls(true);
   CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
         "screen transition drops stale mailbox intents");

   if (argc > 2)
   {
      rib_rmlui_show_controls(false);
      FILE *image = std::fopen(argv[2], "wb");
      CHECK(image, "writable thumbnail fixture");
      if (image) { std::fputs("first", image); std::fclose(image); }
      rib_rmlui_set_slot_state(2, true, argv[2]);
      rib_rmlui_render(960, 600);
      const unsigned before = rib_rmlui_test_texture_loads();
      image = std::fopen(argv[2], "wb");
      if (image) { std::fputs("updated image content", image); std::fclose(image); }
      rib_rmlui_set_slot_state(2, true, argv[2]);
      rib_rmlui_render(960, 600);
      CHECK(rib_rmlui_test_texture_loads() > before,
            "overwriting a thumbnail reloads the same file without reopening the menu");
   }

   /* We write the controls scene into menu.rml in the builder. This template
    * still has the placeholder, and nothing in the bridge replaces it. We
    * write the remap in rmlui.c, which is not linked here, so this does not
    * prove that a choice is saved or applied. */
   rib_rmlui_show_controls(true);
   {
      int image_x = 0;
      int image_y = 0;
      const std::string scene(rib_rmlui_test_text("controller-scene"));
      CHECK(scene.find("control-") == std::string::npos,
            "the player template has no generated control callouts");
      CHECK(!rib_rmlui_element_center("controller-image", &image_x, &image_y),
            "the player template has no controller illustration");
      rib_rmlui_wire_device_picker();
      rib_rmlui_set_device_picker(true, "megadrive6");
      CHECK(std::string(rib_rmlui_test_text("controller-scene")) == scene,
            "naming another pad does not redraw the controls scene");
      CHECK(!rib_rmlui_element_center("controller-image", &image_x, &image_y),
            "naming another pad does not add an illustration");
      CHECK(!rib_rmlui_element_center(
            "controls-device-option-megadrive6", &image_x, &image_y),
            "picker options are export markup, not created by the bridge");
   }

   CHECK(RIB_VOLUME_POSITIONS == 10,
         "ten positions, the top one normal");
   CHECK(AUDIO_VOLUME_MAX_DB == 0.0f,
         "the right end is normal, and the control cannot boost past it");
   CHECK(AUDIO_VOLUME_DEFAULT_DB == AUDIO_VOLUME_MAX_DB,
         "the default is the maximum");
   CHECK(AUDIO_VOLUME_STEP_DB * (RIB_VOLUME_POSITIONS - 1)
               == AUDIO_VOLUME_MAX_DB - AUDIO_VOLUME_MIN_DB,
         "the positions are equal steps from quiet to normal");
   CHECK(rib_volume_db_from_fraction(0.0f) == AUDIO_VOLUME_MIN_DB,
         "the left end of the slider is the quietest it goes");
   CHECK(rib_volume_db_from_fraction(1.0f) == AUDIO_VOLUME_MAX_DB,
         "the right end of the slider is normal");
   CHECK(rib_volume_db_from_fraction(rib_volume_fraction_from_db(0.0f)) == 0.0f,
         "normal, the default, round-trips through the slider");
   CHECK(rib_volume_db_from_fraction(-1.0f) == AUDIO_VOLUME_MIN_DB,
         "a drag past the left end stops at the end");
   CHECK(rib_volume_db_from_fraction(2.0f) == AUDIO_VOLUME_MAX_DB,
         "a drag past the right end stops at normal");
   CHECK(rib_volume_quantize_db(-4.0f) == 0.0f,
         "a level near the top snaps to a position, not to the nearest decibel");

   /* We do not read design.cfg in the interaction harness. We declare Options
    * as an export writes it, so showing it shows the screen that a player
    * opens. */
   rib_rmlui_clear_screens();
   rib_rmlui_declare_screen("pause", "pause-panel", "GAME PAUSED",
         "ESC  CONTINUE", "options-back");
   rib_rmlui_declare_screen("options", "options-panel", "OPTIONS",
         "ESC  BACK", "options");
   rib_rmlui_declare_screen("controls", "controls-panel", "CONTROLS",
         "ESC  BACK", "controls");

   /* Every button on the pause row can take focus, and only one at a time.
    *
    * We read the row from the document, not from a fixed table of element
    * ids. With Options enabled the fourth button is `options`, and moving
    * right from SAVE, reaching Options by keyboard or pad, and pressing down
    * from slot 6 must each focus an element. A button added in a design or an
    * export is reachable without any change here. */
   rib_rmlui_show_screen("pause");
   {
      char row[16][64];
      const int count = rib_rmlui_focusables("pause-panel", row, 16);
      bool options_on_the_row = false;
      int index;

      CHECK(count >= 4, "the pause row has the buttons the design drew");
      for (index = 0; index < count; ++index)
         if (std::string(row[index]) == "options")
            options_on_the_row = true;
      CHECK(options_on_the_row,
            "Options is one of the buttons on the pause row");

      for (index = 0; index < count; ++index)
      {
         int other;
         rib_rmlui_focus_element(row[index]);
         CHECK(rib_rmlui_test_has_class(row[index], "focused"),
               "every button on the pause row can be focused");
         for (other = 0; other < count; ++other)
            if (other != index)
               CHECK(!rib_rmlui_test_has_class(row[other], "focused"),
                     "and only one of them at a time");
      }
      CHECK(std::string(rib_rmlui_focused_element()) == row[count - 1],
            "the row remembers which button has it");
   }

   rib_rmlui_show_screen("options");
   {
      int slider_x = 0;
      int slider_y = 0;
      int mute_x = 0;
      int mute_y = 0;
      CHECK(rib_rmlui_element_center("volume-level", &slider_x, &slider_y),
            "Options has the design's slider");
      CHECK(!rib_rmlui_element_center("volume-mute", &mute_x, &mute_y),
            "there is no mute button");
      rib_rmlui_clear_intents();
      rib_rmlui_pointer_move(slider_x, slider_y);
      rib_rmlui_pointer_button(true);
      rib_rmlui_pointer_move(0, 0);
      rib_rmlui_pointer_button(false);
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SLIDER,
            "dragging off a slider still sets the level");
      CHECK(std::string(rib_rmlui_changed_part()) == "volume-level",
            "the slider reports which part moved");
      CHECK(rib_rmlui_changed_fraction() == 0.0f,
            "a drag off the left end is the bottom of the range");

      rib_rmlui_set_slider("volume-level", 0.5f, nullptr);
      rib_rmlui_clear_intents();
      CHECK(!rib_rmlui_nudge_slider("volume-level", 1),
            "a slider with no step does not move, so a key cannot invent one");
      rib_rmlui_set_slider_step("volume-level", 0.1f);
      CHECK(rib_rmlui_nudge_slider("volume-level", 1), "a key nudges the focused slider");
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SLIDER,
            "the nudge is the same change a drag commits");
      CHECK(rib_rmlui_changed_fraction() > 0.59f && rib_rmlui_changed_fraction() < 0.61f,
            "the nudge adds the slider's own step, not a volume-shaped one");

      rib_rmlui_set_slider("volume-level", 1.0f, nullptr);
      rib_rmlui_set_slider_step("volume-level",
            AUDIO_VOLUME_STEP_DB / (AUDIO_VOLUME_MAX_DB - AUDIO_VOLUME_MIN_DB));
      rib_rmlui_clear_intents();
      click_id("volume-down");
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SLIDER,
            "the left arrow is the slider moving down one position");
      CHECK(rib_rmlui_changed_fraction() > 0.88f && rib_rmlui_changed_fraction() < 0.90f,
            "one arrow is one position, not a decibel");

      /* With Options open and the slider selected, Down skips the left and
       * right arrows of the slider. Left and Right on the slider already step
       * it, so those arrows are for the pointer and are not stops. Down moves
       * to the next focusable element, as for the player. */
      {
         char ids[16][64];
         const int count = rib_rmlui_focusables("options-panel", ids, 16);
         int slider = -1;
         for (int index = 0; index < count; ++index)
            if (std::strcmp(ids[index], RIB_VOLUME_SLIDER_ID) == 0)
               slider = index;
         CHECK(slider >= 0, "the volume slider is a focus stop");
         const char *landed = (slider >= 0 && slider + 1 < count)
               ? ids[slider + 1] : "";
         const char *again = (slider >= 0 && slider + 2 < count)
               ? ids[slider + 2] : "";
         char message[192];
         std::snprintf(message, sizeof(message),
               "pressing down from the slider lands on %s", landed);
         CHECK(std::strcmp(landed, "controls") == 0, message);
         std::snprintf(message, sizeof(message),
               "pressing down again lands on %s", again);
         CHECK(std::strcmp(again, RIB_VOLUME_UP_ID) != 0, message);
         int arrow_x = 0;
         int arrow_y = 0;
         CHECK(rib_rmlui_element_center(RIB_VOLUME_DOWN_ID, &arrow_x, &arrow_y),
               "the left arrow is still there for a pointer");
         CHECK(rib_rmlui_element_center(RIB_VOLUME_UP_ID, &arrow_x, &arrow_y),
               "the right arrow is still there for a pointer");
      }

      /* One move cue per step that changes the level, and none at an end
       * where it does not move. The words come from the pack: up and down. */
      rib_rmlui_set_slider(RIB_VOLUME_SLIDER_ID, 1.0f, nullptr);
      rib_rmlui_set_slider_step(RIB_VOLUME_SLIDER_ID,
            AUDIO_VOLUME_STEP_DB / (AUDIO_VOLUME_MAX_DB - AUDIO_VOLUME_MIN_DB));
      move_sound_log.clear();
      rib_rmlui_nudge_slider(RIB_VOLUME_SLIDER_ID, 1);
      rib_rmlui_nudge_slider(RIB_VOLUME_SLIDER_ID, -1);
      rib_rmlui_nudge_slider(RIB_VOLUME_SLIDER_ID, 1);
      rib_rmlui_set_slider(RIB_VOLUME_SLIDER_ID, 0.0f, nullptr);
      rib_rmlui_nudge_slider(RIB_VOLUME_SLIDER_ID, -1);
      {
         char message[256];
         std::snprintf(message, sizeof(message),
               "volume steps play the move sound once each and not at the ends: heard '%s'",
               move_sound_log.c_str());
         CHECK(move_sound_log == "down up", message);
      }
      drain_actions();
   }

   // Letting go somewhere else must not press the button.
   //
   // In the recorded interaction scenario we press and release at the same
   // point, which is the easy half. This test covers the other half.
   {
      // Earlier checks can leave the controls screen up, where SAVE is hidden
      // and nothing can be clicked, so we set the screen explicitly.
      rib_rmlui_show_screen("pause");
      drain_actions();
      press_then_release_at("save", 4, 4);
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_NONE,
            "pressing a button and releasing off it does nothing");

      // The other half, to show that this does not pass because clicks have
      // stopped working: a press and release on the same button still acts.
      drain_actions();
      click_id("save");
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SAVE,
            "pressing and releasing on a button still presses it");

      // Sliding off and back on is a press, because the release happens on the
      // element where the press began.
      drain_actions();
      {
         int x = 0;
         int y = 0;
         CHECK(rib_rmlui_element_center("save", &x, &y), "element has a hit centre");
         rib_rmlui_pointer_move(x, y);
         rib_rmlui_pointer_button(true);
         rib_rmlui_pointer_move(4, 4);
         rib_rmlui_pointer_move(x, y);
         rib_rmlui_pointer_button(false);
      }
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_SAVE,
            "sliding off a button and back on still presses it");
      drain_actions();
   }

   // Every intent that we can queue in the menu plays a sound, unless we chose
   // silence for it on purpose, so an action added later cannot be silent
   // without a test failure. Changing screen, for example, must still play
   // the confirm cue of the menu.
   {
      const int silent[] = {
         RIB_RMLUI_ACTION_NONE,
         /* A step plays the move cue of the pack when the level changes, not
          * the confirm cue, so the two never play together, even at an end. */
         RIB_RMLUI_ACTION_SLIDER,
         RIB_RMLUI_ACTION_SELECT_SLOT_1, RIB_RMLUI_ACTION_SELECT_SLOT_2,
         RIB_RMLUI_ACTION_SELECT_SLOT_3, RIB_RMLUI_ACTION_SELECT_SLOT_4,
         RIB_RMLUI_ACTION_SELECT_SLOT_5, RIB_RMLUI_ACTION_SELECT_SLOT_6,
      };
      for (int action = RIB_RMLUI_ACTION_NONE;
            action <= RIB_RMLUI_ACTION_SHOW_SCREEN; ++action)
      {
         bool expected_silent = false;
         for (int quiet : silent)
            if (quiet == action)
               expected_silent = true;
         const bool is_silent =
            rib_rmlui_action_sound(action) == RIB_MENU_SOUND_NONE;
         CHECK(is_silent == expected_silent,
               "every intent is audible unless silence was chosen for it");
      }
      CHECK(rib_rmlui_action_sound(RIB_RMLUI_ACTION_SHOW_SCREEN)
            == RIB_MENU_SOUND_OK, "changing screen is confirmed, not silent");
      CHECK(rib_rmlui_action_sound(RIB_RMLUI_ACTION_CONTROLS_BACK)
            == RIB_MENU_SOUND_CANCEL, "leaving a screen cancels, not confirms");
   }

   // We draw an overlay over a running game, and how it looks is up to the
   // design. In the player we only move an element between three states and
   // state whether the menu is on screen. If the rules in the design did not
   // act on that, the notice would appear and vanish without the arriving,
   // leaving or hiding animations in the design.
   {
      rib_rmlui_set_overlay("notice", RIB_OVERLAY_HIDDEN);
      rib_rmlui_set_overlay_mode(true);
      rib_rmlui_render(960, 600);
      const std::string away = rib_rmlui_test_property("notice", "opacity");
      const std::string resting = rib_rmlui_test_property("notice", "bottom");
      CHECK(std::string(rib_rmlui_test_property("footer", "display")) == "none",
            "the design puts the menu away while only overlays are drawn");

      rib_rmlui_set_overlay("notice", RIB_OVERLAY_SHOWING);
      settle(0.6);
      const std::string shown = rib_rmlui_test_property("notice", "opacity");
      CHECK(shown != away, "showing an overlay makes the design draw it");
      CHECK(std::string(rib_rmlui_test_property("notice", "bottom")) != resting,
            "the design moves the notice into place, not only fades it in");

      rib_rmlui_set_overlay("notice", RIB_OVERLAY_LEAVING);
      rib_rmlui_render(960, 600);
      CHECK(std::string(rib_rmlui_test_property("notice", "opacity")) != away,
            "leaving is a transition, not a cut: the first frame is still drawn");
      settle(1.0);
      CHECK(std::string(rib_rmlui_test_property("notice", "opacity")) == away,
            "the design takes the notice away over its own declared time");

      rib_rmlui_set_overlay("notice", RIB_OVERLAY_HIDDEN);
      rib_rmlui_set_overlay_mode(false);
      rib_rmlui_render(960, 600);
      CHECK(std::string(rib_rmlui_test_property("footer", "display")) != "none",
            "the menu comes back when it is what is on screen");
   }

   // The controls of a list screen come after its rows, so moving down with
   // the keyboard past the last row reaches the switch and then BACK, and the
   // player can flip the switch without a pointer.
   {
      // We find the screen button on the pause row instead of naming it. With
      // Options in a game it is not the controls button, so with a fixed name
      // a pad would open Controls instead of Options.
      CHECK(std::string(rib_rmlui_pause_screen_button()) == "options",
            "the pause row's screen button is the one the document has");
      rib_rmlui_declare_screen("fixture", "fixture-panel", "LIST", "ESC  BACK", "");
      CHECK(rib_rmlui_show_screen("fixture"), "a declared list screen shows");
      rib_rmlui_wire_lists();
      rib_rmlui_wire_toggles();
      drain_actions();
      CHECK(rib_rmlui_visible_row_count() == 2,
            "the generated list reports its rows");
      CHECK(rib_rmlui_list_control_count() == 2,
            "the list screen reports its switch and its back button");
      CHECK(std::string(rib_rmlui_list_control_id(0)) == "fixture-mode",
            "the switch comes first, as it is drawn");
      CHECK(std::string(rib_rmlui_list_control_id(1)) == "fixture-back",
            "back comes after it");
      CHECK(std::string(rib_rmlui_list_control_id(2)).empty(),
            "asking past the end names nothing");
      rib_rmlui_focus_list_control(1);
      click_id("fixture-mode");
      CHECK(rib_rmlui_take_action() == RIB_RMLUI_ACTION_TOGGLE,
            "pressing a switch is the general toggle intent");
      CHECK(std::string(rib_rmlui_chosen_item()) == "fixture-mode",
            "which switch travels beside the action");
      rib_rmlui_set_toggle("fixture-mode", "ON", true);
      CHECK(std::string(rib_rmlui_test_text("fixture-mode-state")) == "ON",
            "the switch shows the word the design gave it");
      drain_actions();
   }

   rib_rmlui_shutdown();
   if (failures)
   {
      std::fprintf(stderr, "%d check(s) failed\n", failures);
      return 1;
   }
   std::printf("ok\n");
   return 0;
}
