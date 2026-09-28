/* Headless regression checks against the actual RmlUi bridge and domain helpers.
 * We compile rmlui_bridge.cpp with a dummy renderer and create no window. */

#include "rmlui_bridge.h"
#include "rmlui/view.hpp"
#include "rmlui/script.hpp"
#include "rmlui/binds_popup.hpp"
#include "rmlui/overlays.hpp"
#include "menu_test_view.hpp"
#include "rmlui/sounds.hpp"
#include "rmlui/focus.hpp"
static rib::View view;
/* The font every design staged here declares in design.cfg. */
static const std::vector<std::string> kFonts = {"Silkscreen-Regular.ttf"};
static rib::test::Inspection inspect(view.document);

/* A screen as design.cfg declares it. */
static rib::ScreenDeclaration screen(const char *id, const char *panel,
      const char *heading, const char *footer, const char *button)
{
   rib::ScreenDeclaration declared;
   declared.id = id;
   declared.panel = panel;
   declared.heading = heading;
   declared.footer = footer;
   if (*button)
      declared.buttons = {button};
   return declared;
}

/* Pause, Options and Controls declared as in a Native export, through the
 * menu declaration. We read no design.cfg in this program, and its
 * declarations test stands in for the libretro config reader. */
static void declare_native_screens()
{
   view.screens.clear_screens();
   view.screens.declare_screen(screen("pause", "pause-panel", "GAME PAUSED",
         "ESC  CONTINUE", "options-back"));
   view.screens.declare_screen(screen("options", "options-panel", "OPTIONS",
         "ESC  BACK", "options"));
   view.screens.declare_screen(screen("controls", "controls-panel", "CONTROLS",
         "ESC  BACK", "controls"));
}

/* The ids of the stops in `panel`, in document order. */
static std::vector<std::string> stops_in(const char *panel)
{
   std::vector<std::string> ids;
   for (Rml::Element *stop : view.focus.stops(view.document.root()->GetElementById(panel)))
      ids.push_back(stop->GetId());
   return ids;
}

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include "test_arguments.h"
#include <streams/file_stream.h>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>
// The document fixture has the same typed declaration catalog as Menu.
// With twenty-four declared controls we test the larger controller layout.
static rib_controls_catalog fixture_controls = [] {
   rib_controls_catalog catalog{};
   const char *ids[] = {
      "up", "down", "left", "right", "a", "b", "x", "y",
      "l", "r", "l2", "r2", "l3", "r3", "start", "select",
      "l_x_plus", "l_x_minus", "l_y_plus", "l_y_minus",
      "r_x_plus", "r_x_minus", "r_y_plus", "r_y_minus"
   };
   for (const char *id : ids)
      catalog.entries[catalog.count++].id = id;
   catalog.device_count = 2;
   catalog.devices[0] = {"megadrive", "Mega Drive", 0};
   catalog.devices[1] = {"megadrive6", "Mega Drive six-button", 0};
   return catalog;
}();

static int failures = 0;

#define CHECK(cond, msg) \
   do { \
      if (!(cond)) { \
         std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, msg); \
         ++failures; \
      } \
   } while (0)

/* The accent is on .list-row, so a shader row (a picture) and a disc row
 * (one line) are the same button. Focusing one must not move its name. */
static void check_focus_leaves_the_name(void)
{
   view.document.set_shown("fixture-panel", true);
   struct Pair { int index; const char *focused; const char *rest; const char *which; };
   const Pair pairs[] = {
      {0, "fixture-one-title", "fixture-rest-title", "one-line row"},
      {2, "fixture-two-title", "fixture-pic-title", "picture row"},
   };
   for (const Pair &pair : pairs)
   {
      const char *rows[] = {"fixture-one", "fixture-rest", "fixture-two", "fixture-pic"};
      CHECK(view.focus.set(rows[pair.index]), "a list row takes focus");
      int focused_x = 0, focused_y = 0, focused_w = 0, focused_h = 0;
      int rest_x = 0, rest_y = 0, rest_w = 0, rest_h = 0;
      CHECK(view.document.element_box(pair.focused, &focused_x, &focused_y, &focused_w, &focused_h),
            "the focused row's name has a box");
      CHECK(view.document.element_box(pair.rest, &rest_x, &rest_y, &rest_w, &rest_h),
            "the unfocused row's name has a box");
      char message[256];
      std::snprintf(message, sizeof(message),
            "a focused %s name starts at %d and the other row's name at %d",
            pair.which, focused_x, rest_x);
      CHECK(std::abs(focused_x - rest_x) <= 1, message);
   }
}

static void click_id(const char *id)
{
   int x = 0;
   int y = 0;
   CHECK(view.document.element_center(id, &x, &y), "element has a hit centre");
   view.pointer_move(x, y);
   view.pointer_button(true);
   view.pointer_button(false);
}

static void move_to_id(const char *id)
{
   int x = 0;
   int y = 0;
   CHECK(view.document.element_center(id, &x, &y), "element has a hover centre");
   view.pointer_move(x, y);
}

// Press on one element and release somewhere else. People do this: they put
// the button down, change their mind, slide off and let go. Nothing should
// happen.
static void press_then_release_at(const char *id, int x, int y)
{
   int from_x = 0;
   int from_y = 0;
   CHECK(view.document.element_center(id, &from_x, &from_y), "element has a hit centre");
   view.pointer_move(from_x, from_y);
   view.pointer_button(true);
   view.pointer_move(x, y);
   view.pointer_button(false);
}

static void drain_actions(void)
{
   while (view.intents.take().kind != RIB_RMLUI_ACTION_NONE)
      ;
}

/* Draw for this long. RmlUi advances an animation by at most a tenth of a
 * second per update, so a transition finishes only if we draw frames while
 * the clock moves, as at sixty frames a second in a running game. */
static void settle(double seconds)
{
   for (double at = 0; at < seconds; at += 0.05)
   {
      inspect.advance(0.05);
      view.render(960, 600);
   }
}

struct Box { int x, y, w, h; bool ok; };

static Box box_of(const char *id)
{
   Box box{};
   box.ok = inspect.box(id, &box.x, &box.y, &box.w, &box.h);
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
   const int rows = view.lists.rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const std::string id_row = view.lists.row_in("control-binds", index); const char *id = id_row.c_str();
      if (!id || !*id)
         break;
      if (index < 2)
      {
         /* The long line is what a fixed width would size every list for: a
          * list of "UP / UP / KEY" would be as wide as "LEFT STICK UP / BUTTON 12". */
         view.lists.set_row_text(id,
               index == 0 ? "LEFT STICK UP" : "UP",
               index == 0 ? "BUTTON 12" : "HAT #0 UP",
               index == 0 ? "AXIS" : "PAD");
         view.document.set_shown(id, true);
      }
      else
         view.document.set_shown(id, false);
   }
   view.lists.retarget_pages("control-binds");
}

static int anchors(const char *class_name, std::vector<std::string> &out)
{
   const int count = inspect.class_count(class_name);
   for (int index = 0; index < count; ++index)
   {
      const char *id = inspect.class_id(class_name, index);
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
   const int rows = view.lists.rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const std::string id_row = view.lists.row_in("control-binds", index); const char *id = id_row.c_str();
      if (!id || !*id)
         break;
      if (index < (int)matched.size())
      {
         view.lists.set_row_text(id, matched[index]->title.c_str(),
               matched[index]->detail.c_str(), matched[index]->kind.c_str());
         view.document.set_shown(id, true);
      }
      else
         view.document.set_shown(id, false);
   }
   view.lists.retarget_pages("control-binds");
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
   /* We paint the focused callout as we paint every focused stop. */
   std::snprintf(message, sizeof(message), "%s: %s takes focus", design, focused.c_str());
   CHECK(view.focus.set(focused.c_str()), message);
   /* test_property uses one buffer, so we copy the value before the next
    * read. */
   const std::string focused_z = inspect.property(focused.c_str(), "z-index");
   const std::string neighbour_z = inspect.property(neighbour.c_str(), "z-index");
   std::snprintf(message, sizeof(message),
         "%s: focused %s z-index is '%s' and %s is '%s'; the focused control is under its neighbour",
         design, focused.c_str(), focused_z.c_str(),
         neighbour.c_str(), neighbour_z.c_str());
   CHECK(stacking_rank(focused_z.c_str()) > stacking_rank(neighbour_z.c_str()), message);

   move_to_id(neighbour.c_str());
   const std::string hover_z = inspect.property(neighbour.c_str(), "z-index");
   std::snprintf(message, sizeof(message),
         "%s: hovered %s z-index is '%s'; the control under the pointer is under its neighbour",
         design, neighbour.c_str(), hover_z.c_str());
   CHECK(stacking_rank(hover_z.c_str()) > 0, message);

   if (view.document.has_element("control-group-l_stick"))
   {
      CHECK(view.focus.set("control-group-l_stick"), "the stick group takes focus");
      const std::string group_z = inspect.property(
            "control-group-l_stick", "z-index");
      std::snprintf(message, sizeof(message),
            "%s: focused stick group z-index is '%s'",
            design, group_z.c_str());
      CHECK(stacking_rank(group_z.c_str()) > 0, message);
   }

   view.pointer_move(1, 1);
}

/* The leader runs of a stop on the pad. We write them just before the stop,
 * in the scene and not inside the stop, so we draw the stop over their end
 * and a pointer on a run is on no stop. */
static std::vector<Rml::Element*> leaders_of(Rml::Element *stop)
{
   std::vector<Rml::Element*> leaders;
   for (Rml::Element *at = stop->GetPreviousSibling();
         at && at->IsClassSet("control-leader"); at = at->GetPreviousSibling())
      leaders.insert(leaders.begin(), at);
   return leaders;
}

/* The ring of a stop over its button on the pad, inside the stop, so it
 * lights when the stop has focus and a pointer on it is on the stop. */
static Rml::Element *ring_of(Rml::Element *stop)
{
   Rml::Element *ring = nullptr;
   rib::walk(stop, [&](Rml::Element *element) {
      if (element->IsClassSet("control-hit"))
         ring = element;
      return rib::Walk::Continue;
   });
   return ring;
}

/* The leader runs of a stop, then its ring, in the order of scene-geometry. */
static std::vector<Rml::Element*> marks_of(Rml::Element *stop)
{
   std::vector<Rml::Element*> marks = leaders_of(stop);
   if (Rml::Element *ring = ring_of(stop))
      marks.push_back(ring);
   return marks;
}

/* Where the element is drawn in the layout as it stands: call settle()
 * after anything that changes it. */
static Rml::Vector2f placed_at(Rml::Element *element)
{
   return element->GetAbsoluteOffset(Rml::BoxArea::Border);
}

static void settle()
{
   view.document.get_context()->Update();
}

static Box drawn_box(Rml::Element *element)
{
   const Rml::Vector2f at = placed_at(element);
   const Rml::Vector2f size = element->GetBox().GetSize(Rml::BoxArea::Border);
   return {(int)std::floor(at.x), (int)std::floor(at.y),
         (int)std::ceil(at.x + size.x) - (int)std::floor(at.x),
         (int)std::ceil(at.y + size.y) - (int)std::floor(at.y), true};
}

/* Two boxes that share an edge without overlapping. */
static bool boxes_touch(const Box &a, const Box &b)
{
   const bool across = a.y < b.y + b.h && b.y < a.y + a.h;
   const bool along = a.x < b.x + b.w && b.x < a.x + a.w;
   return (across && (a.x + a.w == b.x || b.x + b.w == a.x))
         || (along && (a.y + a.h == b.y || b.y + b.h == a.y));
}

static std::vector<Rml::Element*> scene_leaders()
{
   std::vector<Rml::Element*> leaders;
   rib::walk(view.document.root()->GetElementById("controller-scene"), [&](Rml::Element *element) {
      if (element->IsClassSet("control-leader"))
         leaders.push_back(element);
      return rib::Walk::Continue;
   });
   return leaders;
}

/* Where `scene_layout` puts the leader runs and ring of each stop on the
 * scene, in its order: the `.marks` file next to the scene, a line per mark
 * with the id, x and y of the stop, from the exporter's scene-geometry. We
 * check them in the RmlUi layout, so we also measure the border of the stop,
 * from which we place its ring. */
static void check_marks_where_the_layout_puts_them(const std::filesystem::path &scene,
      const char *design, const char *profile)
{
   std::filesystem::path path = scene;
   path.replace_extension(".marks");
   std::ifstream in(path);
   char message[512];
   std::snprintf(message, sizeof(message), "%s/%s: no %s", design, profile, path.string().c_str());
   CHECK(in.good(), message);
   struct Mark { std::string stop; Rml::Vector2f at, size; };
   std::vector<Mark> expected;
   for (std::string row; std::getline(in, row); )
   {
      std::stringstream fields(row);
      std::string stop;
      float x = 0.f, y = 0.f, w = 0.f, h = 0.f;
      if (std::getline(fields, stop, '\t') && fields >> x >> y >> w >> h)
         expected.push_back({stop, {x, y}, {w, h}});
   }
   Rml::Element *scene_element = view.document.root()->GetElementById("controller-scene");
   settle();
   const Rml::Vector2f origin = placed_at(scene_element);
   size_t at = 0;
   std::vector<std::string> stops;
   for (const auto &mark : expected)
      if (stops.empty() || stops.back() != mark.stop)
         stops.push_back(mark.stop);
   for (const std::string &stop_id : stops)
   {
      Rml::Element *stop = view.document.root()->GetElementById(stop_id);
      std::snprintf(message, sizeof(message), "%s/%s: the scene has no %s", design, profile, stop_id.c_str());
      CHECK(stop != nullptr, message);
      const std::vector<Rml::Element*> marks = stop ? marks_of(stop) : std::vector<Rml::Element*>{};
      for (Rml::Element *mark : marks)
      {
         if (at >= expected.size() || expected[at].stop != stop_id)
         {
            std::snprintf(message, sizeof(message), "%s/%s: %s draws more marks than the layout gives it",
                  design, profile, stop_id.c_str());
            CHECK(false, message);
            break;
         }
         const Rml::Vector2f drawn = placed_at(mark) - origin;
         std::snprintf(message, sizeof(message),
               "%s/%s: a mark of %s is drawn at %.1f,%.1f on the scene; the layout puts it at %.1f,%.1f",
               design, profile, stop_id.c_str(), drawn.x, drawn.y,
               expected[at].at.x, expected[at].at.y);
         CHECK(drawn == expected[at].at, message);
         /* We draw a ring at the size from the layout, so it is centred on its
          * button. A border drawn outside that size would move the centre. The
          * thickness of a leader run comes from the design. */
         if (mark->IsClassSet("control-hit"))
         {
            const Rml::Vector2f size = mark->GetBox().GetSize(Rml::BoxArea::Border);
            std::snprintf(message, sizeof(message),
                  "%s/%s: the ring of %s is drawn %.1fx%.1f; the layout gives it %.1fx%.1f",
                  design, profile, stop_id.c_str(), size.x, size.y,
                  expected[at].size.x, expected[at].size.y);
            CHECK(size == expected[at].size, message);
         }
         ++at;
      }
      while (at < expected.size() && expected[at].stop == stop_id)
      {
         std::snprintf(message, sizeof(message), "%s/%s: %s draws fewer marks than the layout gives it",
               design, profile, stop_id.c_str());
         CHECK(false, message);
         ++at;
      }
   }
}

/* The ring of a control is part of the stop for that control, and its leader
 * is not. When the stop has focus we light the ring and move nothing in the
 * stop. The leader is outside the stop and ends at its outer edge. We draw a
 * focused stop over every leader, its own included, so no leader shows in
 * its box. We draw the ring outside the box of the stop, and a pointer at
 * its centre, on the pad, is on the stop. */
static void check_marks_belong_to_their_stop(const char *design, const char *profile)
{
   std::vector<std::string> stops;
   anchors("control-callout", stops);
   anchors("control-group", stops);
   char message[512];
   int rings = 0;
   int outside = 0;
   /* A list left open by an earlier check is above the scene. */
   view.document.set_shown("control-binds", false);
   settle();
   std::vector<Box> every_run;
   for (Rml::Element *leader : scene_leaders())
      every_run.push_back(drawn_box(leader));
   for (const std::string &stop_id : stops)
   {
      Rml::Element *stop = view.document.root()->GetElementById(stop_id);
      Rml::Element *ring = ring_of(stop);
      if (!ring)
         continue;
      ++rings;
      const std::vector<Rml::Element*> leaders = leaders_of(stop);
      const std::vector<Rml::Element*> marks = marks_of(stop);
      bool leader_inside = false;
      rib::walk(stop, [&](Rml::Element *element) {
         leader_inside = leader_inside || element->IsClassSet("control-leader");
         return rib::Walk::Continue;
      });
      std::snprintf(message, sizeof(message), "%s/%s: %s has a leader run inside it",
            design, profile, stop_id.c_str());
      CHECK(!leader_inside, message);
      std::snprintf(message, sizeof(message), "%s/%s: %s has no leader written before it",
            design, profile, stop_id.c_str());
      CHECK(!leaders.empty(), message);

      const std::string ring_id = ring->GetId();
      CHECK(view.focus.set("controls-back"), "Back takes focus");
      const std::string unlit = inspect.property(ring_id.c_str(), "border-top-color");
      std::vector<Rml::Vector2f> resting;
      for (Rml::Element *mark : marks)
         resting.push_back(placed_at(mark));
      const Box stop_box = box_of(stop_id.c_str());
      bool meets = false;
      for (Rml::Element *leader : leaders)
      {
         const Box run = drawn_box(leader);
         std::snprintf(message, sizeof(message),
               "%s/%s: a leader run of %s at %d,%d %dx%d lies on its box %d,%d %dx%d",
               design, profile, stop_id.c_str(), run.x, run.y, run.w, run.h,
               stop_box.x, stop_box.y, stop_box.w, stop_box.h);
         CHECK(!boxes_overlap(run, stop_box), message);
         meets = meets || boxes_touch(run, stop_box);
      }
      std::snprintf(message, sizeof(message),
            "%s/%s: no leader run of %s ends at its outer edge %d,%d %dx%d",
            design, profile, stop_id.c_str(), stop_box.x, stop_box.y, stop_box.w, stop_box.h);
      CHECK(meets, message);

      std::snprintf(message, sizeof(message), "%s/%s: %s takes focus",
            design, profile, stop_id.c_str());
      CHECK(view.focus.set(stop_id.c_str()), message);
      const std::string lit = inspect.property(ring_id.c_str(), "border-top-color");
      std::snprintf(message, sizeof(message),
            "%s/%s: #%s is %s while %s has focus and %s while it does not",
            design, profile, ring_id.c_str(), lit.c_str(), stop_id.c_str(), unlit.c_str());
      CHECK(lit != unlit, message);
      for (size_t index = 0; index < marks.size(); ++index)
      {
         const Rml::Vector2f now = placed_at(marks[index]);
         std::snprintf(message, sizeof(message),
               "%s/%s: a mark of %s moves from %.1f,%.1f to %.1f,%.1f when it takes focus",
               design, profile, stop_id.c_str(), resting[index].x, resting[index].y, now.x, now.y);
         CHECK(now == resting[index], message);
      }
      /* What is on top at each point a leader shares with the focused box
       * is what is drawn there. */
      const Box focused_box = box_of(stop_id.c_str());
      int shown = 0;
      Box where{};
      for (const Box &run : every_run)
      {
         for (int y = std::max(run.y, focused_box.y); y < std::min(run.y + run.h, focused_box.y + focused_box.h); ++y)
            for (int x = std::max(run.x, focused_box.x); x < std::min(run.x + run.w, focused_box.x + focused_box.w); ++x)
            {
               Rml::Element *top = view.document.get_context()->GetElementAtPoint({x + 0.5f, y + 0.5f});
               if (top && top->IsClassSet("control-leader"))
               {
                  if (!shown++)
                     where = {x, y, 1, 1, true};
               }
            }
      }
      std::snprintf(message, sizeof(message),
            "%s/%s: with %s focused, %d leader pixel(s) are drawn inside its box %d,%d %dx%d, first at %d,%d",
            design, profile, stop_id.c_str(), shown,
            focused_box.x, focused_box.y, focused_box.w, focused_box.h, where.x, where.y);
      CHECK(shown == 0, message);

      const Box ring_box = box_of(ring_id.c_str());
      if (!boxes_overlap(stop_box, ring_box))
         ++outside;
      view.pointer_move(ring_box.x + ring_box.w / 2, ring_box.y + ring_box.h / 2);
      Rml::Element *found = view.document.get_context()->GetHoverElement();
      std::snprintf(message, sizeof(message),
            "%s/%s: the pointer at #%s's centre finds %s, which is not on %s",
            design, profile, ring_id.c_str(),
            found ? found->GetAddress(false, false).c_str() : "nothing", stop_id.c_str());
      CHECK(found && found != stop && view.focus.stop_at(found) == stop, message);
   }
   std::snprintf(message, sizeof(message),
         "%s/%s: no ring is drawn outside its stop, so none of this reached one",
         design, profile);
   CHECK(outside > 0 || rings == 0, message);
   view.pointer_move(1, 1);
}

/* On the pad, the pointer reaches a control only inside the ring drawn over
 * its button, and never where the ring of another button is nearer. Rings
 * are square boxes with round corners drawn inside, so neighbouring rings
 * overlap as squares. A pointer between Up and Left on the Mega Drive pad,
 * nearer Up, must reach Up. We do not check points on a callout or on the
 * box of a stick, because they belong to the box. */
static void check_pointer_reaches_a_control_only_on_its_ring(const char *design, const char *profile)
{
   std::vector<std::string> stops;
   anchors("control-callout", stops);
   anchors("control-group", stops);
   struct Ring { Rml::Element *stop; float x, y, radius; Box box; };
   std::vector<Ring> rings;
   std::vector<Box> boxes;
   view.document.set_shown("control-binds", false);
   view.pointer_move(1, 1);
   settle();
   for (const std::string &stop_id : stops)
   {
      Rml::Element *stop = view.document.root()->GetElementById(stop_id);
      boxes.push_back(drawn_box(stop));
      if (Rml::Element *ring = ring_of(stop))
      {
         const Box drawn = drawn_box(ring);
         rings.push_back({stop, drawn.x + drawn.w / 2.f, drawn.y + drawn.h / 2.f, drawn.w / 2.f, drawn});
      }
   }
   char message[512];
   int wrong = 0;
   for (const Ring &ring : rings)
      for (int y = ring.box.y; y < ring.box.y + ring.box.h; y += 3)
         for (int x = ring.box.x; x < ring.box.x + ring.box.w; x += 3)
         {
            const float px = x + 0.5f, py = y + 0.5f;
            bool on_a_box = false;
            for (const Box &box : boxes)
               on_a_box = on_a_box || (x >= box.x && x < box.x + box.w && y >= box.y && y < box.y + box.h);
            if (on_a_box)
               continue;
            Rml::Element *reached = view.focus.stop_at(
                  view.document.get_context()->GetElementAtPoint({px, py}));
            if (!reached)
               continue;
            const Ring *own = nullptr;
            const Ring *nearest = nullptr;
            float nearest_distance = 0.f;
            for (const Ring &other : rings)
            {
               const float distance = std::hypot(px - other.x, py - other.y);
               if (other.stop == reached)
                  own = &other;
               if (!nearest || distance < nearest_distance)
               {
                  nearest = &other;
                  nearest_distance = distance;
               }
            }
            const float own_distance = own ? std::hypot(px - own->x, py - own->y) : 1e9f;
            const bool inside = own && own_distance <= own->radius;
            const bool closest = own && own_distance <= nearest_distance + 1.f;
            if (inside && closest)
               continue;
            if (!wrong++)
            {
               std::snprintf(message, sizeof(message),
                     "%s/%s: the pointer at %d,%d reaches %s, %s",
                     design, profile, x, y, reached->GetId().c_str(),
                     !inside ? "outside the ring drawn over its button"
                             : ("nearer " + nearest->stop->GetId() + "'s ring").c_str());
               CHECK(false, message);
            }
         }
   if (wrong > 1)
   {
      std::snprintf(message, sizeof(message), "%s/%s: and %d more such points on the pad",
            design, profile, wrong - 1);
      CHECK(false, message);
   }
   view.pointer_move(1, 1);
}

static void collect_painted(const Box &list, std::vector<Box> &painted)
{
   painted.clear();
   if (list.ok)
      painted.push_back(list);
   const int row_count = view.lists.rows_in("control-binds");
   for (int index = 0; index < row_count; ++index)
   {
      const std::string row_id_row = view.lists.row_in("control-binds", index); const char *row_id = row_id_row.c_str();
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
   const int row_count = view.lists.rows_in("control-binds");
   for (int index = 0; index < row_count; ++index)
   {
      const std::string row_id_row = view.lists.row_in("control-binds", index); const char *row_id = row_id_row.c_str();
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
   view.lists.place_list("control-binds", anchor, width);
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
      const std::string first_row = view.lists.row_in("control-binds", 0); const char *first = first_row.c_str();
      const std::string title_id = first ? std::string(first) + "-title" : "";
      const std::string title = first ? inspect.text(title_id.c_str()) : "";
      if (!stand_in_noted && title == "LEFT STICK UP")
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
   const int row_count = view.lists.rows_in("control-binds");
   for (int index = 0; index < row_count; ++index)
   {
      const std::string row_id_row = view.lists.row_in("control-binds", index); const char *row_id = row_id_row.c_str();
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
      const std::string row_id_row = view.lists.row_in("control-binds", index); const char *row_id = row_id_row.c_str();
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
   const int rows = view.lists.rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const std::string id_row = view.lists.row_in("control-binds", index); const char *id = id_row.c_str();
      if (!id || !*id || !inspect.row_glyphs_overlap(id))
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
    * long stand-in, the text that a fixed width would have to fit. We place
    * the list for these rows, as for the short ones below, so that its width
    * does not remain from the last scene placed in the sweep. */
   fill_bind_rows();
   view.render(960, 600);
   view.lists.place_list("control-binds", "control-up", declared);
   check_glyphs(design, "long");
   const int rows = view.lists.rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const std::string id_row = view.lists.row_in("control-binds", index); const char *id = id_row.c_str();
      if (!id || !*id)
         break;
      if (index < 2)
         view.lists.set_row_text(id, "A", "BUTTON 2", "PAD");
      else
         view.document.set_shown(id, false);
   }
   view.lists.retarget_pages("control-binds");
   view.render(960, 600);
   view.lists.place_list("control-binds", "control-up", declared);
   const std::string prop = inspect.property("control-binds", "width");
   int parsed = 0;
   std::sscanf(prop.c_str(), "%d", &parsed);
   char message[256];
   std::snprintf(message, sizeof(message),
         "%s short list width is %s; two short words should be under 200dp (declared %d)",
         design, prop.c_str(), declared);
   CHECK(parsed > 0 && parsed < 200 && parsed <= declared, message);
   check_glyphs(design, "short");
}

static void check_volume_ends(const char *design, int window_w, int window_h)
{
   char message[384];
   const struct { float fraction; const char *end; const char *arrow; } ends[] = {
      {0.f, "quiet", "volume-down"},
      {1.f, "normal", "volume-up"},
   };
   for (const auto &end : ends)
   {
      view.parts.set_slider("volume-level", end.fraction, nullptr);
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
   const int rows = view.lists.rows_in("control-binds");
   for (int index = 0; index < rows; ++index)
   {
      const std::string id_row = view.lists.row_in("control-binds", index); const char *id = id_row.c_str();
      if (!id || !*id)
         break;
      if (index < 10)
      {
         view.lists.set_row_text(id, "Right stick up", "Axis -0", "AXIS");
         view.document.set_shown(id, true);
      }
      else
         view.document.set_shown(id, false);
   }
   view.lists.retarget_pages("control-binds");
}

static int check_rstick_picture(const char *design, const char *profile, int width)
{
   if (std::strcmp(profile, "ps1") != 0 && std::strcmp(profile, "ps1-analog") != 0)
      return 0;
   if (!view.document.has_element("control-group-r_stick"))
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
      view.render(size[0], size[1]);
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
/* Disc fills the window: its screen spans every width, and its button
 * column keeps a 24dp margin from the right edge. The canvas is 960x600dp
 * scaled to fit, so at these sizes one dp is one pixel. */
static void check_disc_fills_the_window(const char *design)
{
   if (std::strcmp(design, "disc") != 0)
      return;
   view.screens.show_screen("pause");
   for (const int width : {960, 1280})
   {
      char message[160];
      view.render(width, 600);
      const Box screen = box_of("screen");
      const Box actions = box_of("actions");
      std::snprintf(message, sizeof(message),
            "disc %dx600: #screen spans the window (x %d, width %d)", width, screen.x, screen.w);
      CHECK(screen.x == 0 && screen.w == width, message);
      std::snprintf(message, sizeof(message),
            "disc %dx600: #actions ends 24dp from the right edge (ends at %d)", width, actions.x + actions.w);
      CHECK(actions.w > 0 && width - (actions.x + actions.w) == 24, message);
   }
   view.render(960, 600);
}

static int check_placement(const char *assets, const char *scenes,
      const char *design, int width)
{
   if (!view.initialize(assets, kFonts, 960, 600, false, fixture_controls))
   {
      std::fprintf(stderr, "FAIL could not init RmlUi from %s\n", assets);
      return 1;
   }
   declare_native_screens();

   /* 1920x1200 is a 960x600 window on a 2x display, the size at which the
    * right border of the list is on the last pixel. */
   const int sizes[][2] = {{960, 600}, {1440, 900}, {1920, 1200}};
   view.screens.show_screen("options");
   for (const auto &size : sizes)
   {
      view.render(size[0], size[1]);
      check_volume_ends(design, size[0], size[1]);
   }

   view.screens.show_screen("controls");
   fill_bind_rows();
   int scenes_seen = 0;
   /* In name order: a directory lists its files in whatever order its
    * filesystem keeps them, and a check must not depend on that. */
   std::vector<std::filesystem::directory_entry> entries;
   for (const auto &entry : std::filesystem::directory_iterator(scenes))
      entries.push_back(entry);
   std::sort(entries.begin(), entries.end());
   for (const auto &entry : entries)
   {
      if (entry.path().extension() != ".rml")
         continue;
      const std::string profile = entry.path().stem().string();
      std::ifstream in(entry.path());
      std::stringstream buffer;
      buffer << in.rdbuf();
      const std::string markup = buffer.str();
      if (!view.controls.set_scene(profile.c_str(), markup.c_str()))
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
      view.render(960, 600);
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
      check_marks_where_the_layout_puts_them(entry.path(), design, profile.c_str());
      check_marks_belong_to_their_stop(design, profile.c_str());
      check_pointer_reaches_a_control_only_on_its_ring(design, profile.c_str());
      for (const auto &size : sizes)
      {
         view.render(size[0], size[1]);
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
   check_disc_fills_the_window(design);
   {
      view.screens.show_screen("pause");
      view.focus.set("save");
      view.status.set_main("A \"quoted\" status\\path\nline");
      view.render(960, 600);
      drain_actions();
      const std::string report = rib::Script(view).report(
            "pause", true, false, false, "megadrive", -12.0f);
      CHECK(report.find("\"focused\":[\"save\"]") != std::string::npos,
            "the checkpoint observes the actual visible focus");
      CHECK(report.find("A &quot;quoted&quot; status\\\\path\\u000aline") != std::string::npos,
            "the checkpoint preserves serialized RML and JSON-escapes its backslash and newline");
      CHECK(report.find("\"profile\":\"megadrive\",\"volumeDb\":-12.000000") != std::string::npos,
            "the checkpoint carries the runtime profile and volume");
      CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_NONE,
            "observation does not generate a menu action");
   }
   view.shutdown();
   if (failures)
   {
      std::fprintf(stderr, "%d check(s) failed\n", failures);
      return 1;
   }
   return 0;
}

int test_menu_declarations();
int test_menu_named_folder(const char *assets, const std::vector<std::string>& fonts,
      const rib_controls_catalog& controls);

int main(int argc, char **argv)
{
   /* Paths arrive as UTF-8 on every platform. */
   Utf8Arguments utf8(argc, argv);
   argc = utf8.argc();
   argv = utf8.argv();
   if (argc == 2 && std::strcmp(argv[1], "declarations") == 0)
      return test_menu_declarations();
   const char *assets = argc > 1 ? argv[1] : nullptr;
   if (!assets || !*assets)
   {
      std::fprintf(stderr, "usage: test_rmlui_interaction ASSET_DIR\n");
      return 2;
   }
   if (argc > 2 && std::strcmp(argv[2], "row-edge") == 0)
   {
      if (!view.initialize(assets, kFonts, 960, 600, false, fixture_controls))
      {
         std::fprintf(stderr, "FAIL could not init RmlUi from %s\n", assets);
         return 1;
      }
      check_focus_leaves_the_name();
      view.shutdown();
      if (failures)
      {
         std::fprintf(stderr, "%d check(s) failed\n", failures);
         return 1;
      }
      std::printf("ok\n");
      return 0;
   }
   if (argc > 2 && std::strcmp(argv[2], "named-folder") == 0)
      return test_menu_named_folder(assets, kFonts, fixture_controls);
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

   CHECK(!rib_rmlui_ok_includes_pointer_select(true),
         "RmlUi OK does not consume the pointer select bit");
   CHECK(!view.slots.occupied(1),
         "empty Load is not actionable");
   CHECK(!rib::state_task_matches(false, true, "/s", 1, "/s", 1, true),
         "no pending operation does not match");
   CHECK(!rib::state_task_matches(true, true, "/s", 1, "/s", 1, false),
         "a load result does not resolve a save");
   CHECK(!rib::state_task_matches(true, true, "/s1", 1, "/s2", 1, true),
         "another path is ignored");
   CHECK(!rib::state_task_matches(true, true, "/s", 1, "/s", 2, true),
         "another slot is ignored");
   CHECK(rib::state_task_matches(true, false, "/s", 3, "/s", 3, false),
         "exact load path and slot match");

   if (!view.initialize(assets, kFonts, 960, 600, false, fixture_controls))
   {
      std::fprintf(stderr, "FAIL could not init RmlUi from %s\n", assets);
      return 1;
   }
   /* The screen button on the pause row is Options, and `controls` is inside
    * that panel. */
   declare_native_screens();

   view.status.set_main("SAVED");
   view.status.set_controls("DEFAULTS RESTORED");
   inspect.advance(4);
   view.render(960, 600);
   CHECK(std::string(inspect.text("status")) == "SAVED", "status remains briefly");
   inspect.advance(2);
   view.render(960, 600);
   {
      /* When a status expires, we put back the prompt from the design. */
      auto prompt = [&](const char *id) {
         Rml::Element *line = view.document.root()->GetElementById(id);
         return line ? line->GetAttribute<Rml::String>("data-prompt", "") : Rml::String();
      };
      CHECK(!prompt("status").empty(), "the design gives the status line a prompt");
      CHECK(std::string(inspect.text("status")) == prompt("status"), "main status expires back to the design's prompt");
      CHECK(std::string(inspect.text("controls-status")) == prompt("controls-status"),
            "controls status expires back to its prompt");
   }
   {
      /* The document contains the aspect ratio of the game, and we give a
       * marked element that ratio within the largest size in its stylesheet.
       * An element with no largest size keeps its box, and a value that is not
       * a ratio changes nothing. The navigation tests check where the slot
       * picture of each design goes, for every game. */
      Rml::Element *root = view.document.root();
      Rml::Element *picture = root->QuerySelector("#slot-1 .slot-picture");
      Rml::Element *status = root->GetElementById("status");
      CHECK(picture && picture->HasAttribute("data-game-shaped"),
            "Native marks its slot picture to take the game's shape");
      auto shape = [&] { return root->GetAttribute<Rml::String>("data-game-shape", ""); };
      auto content = [&](Rml::Element *element) {
         view.render(960, 600);
         return element ? element->GetBox().GetSize(Rml::BoxArea::Content) : Rml::Vector2f();
      };
      const Rml::Vector2f unmarked = content(status);
      status->SetAttribute("data-game-shaped", Rml::String());
      view.document.show_game_shape(16.0f/9);
      CHECK(shape() == "1.778", ("the document states 16:9 as 1.778, not " + shape()).c_str());
      const Rml::Vector2f wide = content(picture);
      CHECK(std::abs(wide.x - 230.0f) < 0.5f && std::abs(wide.y - 129.375f) < 0.5f,
            "a 16:9 picture is as wide as its largest size allows");
      const Rml::Vector2f kept = content(status);
      CHECK(kept.x == unmarked.x && kept.y == unmarked.y,
            "a marked element with no largest size keeps its box");
      for (float nothing : {0.0f, -1.0f, std::nanf(""), 1000.0f})
         view.document.show_game_shape(nothing);
      CHECK(shape() == "1.778", "no shape at all leaves the one shown");
      view.document.show_game_shape(10.0f/9);
      const Rml::Vector2f tall = content(picture);
      CHECK(shape() == "1.111" && std::abs(tall.x - 153.333f) < 0.5f
               && std::abs(tall.y - 138.0f) < 0.5f,
            "a 10:9 picture is as tall as its largest size allows");
      status->RemoveAttribute("data-game-shaped");
      view.document.show_game_shape(4.0f/3);
   }
   view.lists.place_list("fixture-panel", nullptr, 0);
   CHECK(std::string(inspect.property("fixture-panel", "display")) != "none",
         "popup placement accepts its declared element without requiring a list class");
   view.document.set_shown("fixture-panel", false);
   click_id("save");
   click_id("options");
   const auto first = view.intents.take();
   const auto second = view.intents.take();
   CHECK(first.kind == RIB_RMLUI_ACTION_SAVE,
         "mailbox preserves the first click");
   // Changing screen has no separate action. We pass the requested screen
   // next to one shared action, so declaring a screen never adds to the
   // enum. This test is mainly about the order, and it also checks that the
   // id arrived.
   CHECK(second.kind == RIB_RMLUI_ACTION_SHOW_SCREEN,
         "mailbox preserves the following click");
   CHECK(second.id == "options",
         "the screen asked for travels with the action");
   CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_NONE,
         "mailbox is empty after both intents");

   click_id("slot-3");
   click_id("slot-5");
   const auto third_slot = view.intents.take();
   const auto fifth_slot = view.intents.take();
   CHECK(third_slot.kind == RIB_RMLUI_ACTION_SELECT_SLOT && third_slot.slot == 3,
         "first queued slot click carries its own slot");
   CHECK(fifth_slot.kind == RIB_RMLUI_ACTION_SELECT_SLOT && fifth_slot.slot == 5,
         "second queued slot click carries its own slot");

   move_to_id("resume");
   CHECK(view.hovered.kind == RIB_RMLUI_ACTION_RESUME,
         "pointer hover tracks Resume");
   view.pointer_move(8, 8);
   CHECK(view.hovered.kind == RIB_RMLUI_ACTION_NONE,
         "pointer leave clears hover instead of sticking");

   view.focus.set("quit");
   view.slots.set_selected_slot(4);
   move_to_id("resume");
   CHECK(view.hovered.kind == RIB_RMLUI_ACTION_RESUME,
         "hover is independent of keyboard focus");

   /* The focused element is named in the document, and the design marks the
    * slot that SAVE and LOAD use only while one of them has focus. Elsewhere
    * it looks like any slot, so nobody can mistake it for the cursor. */
   view.slots.set_selected_slot(4);
   view.focus.set("resume");
   view.pointer_move(1, 1);
   view.follow_pointer();
   const std::string plain_border = inspect.property("slot-2", "border-top-color");
   CHECK(inspect.property("slot-4", "border-top-color") == plain_border,
         "the chosen slot looks like any other while CONTINUE has focus");
   view.focus.set("save");
   CHECK(view.document.root()->GetAttribute<Rml::String>("data-focus", "") == "save",
         "the document names the focused element");
   view.document.root()->Focus();
   view.focus.paint();
   CHECK(!view.document.root()->HasAttribute("data-focus"),
         "the name goes when nothing has focus");
   view.focus.set("save");
   CHECK(inspect.property("slot-4", "border-top-color") != plain_border,
         "SAVE shows the slot it saves to");
   move_to_id("quit");
   view.follow_pointer();
   CHECK(view.document.root()->GetAttribute<Rml::String>("data-focus", "") == "quit"
         && inspect.property("slot-4", "border-top-color") == plain_border,
         "the pointer leaving SAVE for QUIT hides it again");
   move_to_id("save");
   view.follow_pointer();
   CHECK(inspect.property("slot-4", "border-top-color") != plain_border,
         "the pointer onto SAVE shows it");
   move_to_id("slot-4");
   view.focus.set("slot-4");
   const std::string focused_border = inspect.property("slot-4", "border-top-color");
   CHECK(focused_border != plain_border, "a slot with focus shows it");
   view.pointer_button(true);
   CHECK(focused_border != inspect.property("slot-4", "border-top-color"),
         "slot has pressed feedback while held");
   view.pointer_move(1, 1);
   view.pointer_button(false);

   view.slots.set_slot_state(1, false, nullptr);
   CHECK(inspect.has_class("load", "disabled"),
         "empty Load is disabled");
   view.clear_intents();
   click_id("load");
   CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_NONE,
         "disabled Load does not enqueue an action");

   view.screens.show_screen("controls");
   int control_x = 0, control_y = 0;
   if (view.document.element_center("control-up", &control_x, &control_y)) {
      view.controls.set_control_state("control-up", "up", "Up", "up", true);
      const std::string animation = inspect.property("control-up", "animation");
      CHECK(animation.find("capture-pulse") != std::string::npos, "capture animates the control itself");
      view.render(960, 600);
      const std::string border = inspect.property("control-up", "border-top-color");
      inspect.advance(0.3);
      view.render(960, 600);
      CHECK(border != inspect.property("control-up", "border-top-color"), "capture border changes over time");
      view.controls.set_control_state("control-up", "up", "Up", "up", false);
      CHECK(std::string(inspect.property("control-up", "animation")).find("capture-pulse") == std::string::npos, "capture cue stops when capture ends");
   }
   view.controls.set_capturing(true);
   int cancel_x = 0;
   int cancel_y = 0;
   CHECK(view.document.element_center("controls-cancel", &cancel_x, &cancel_y),
         "Cancel has a hit centre while capture is visible");
   view.clear_intents();
   view.pointer_move(cancel_x, cancel_y);
   view.pointer_button(true);
   view.pointer_button(false);
   CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_CONTROLS_CANCEL,
         "Cancel is consumed by RmlUi before any binder poll");

   for (const char *id : {"controls-cancel", "controls-reset", "controls-back"})
   {
      move_to_id(id);
      const std::string hovered = inspect.property(id, "border-top-color");
      view.pointer_button(true);
      CHECK(hovered != inspect.property(id, "border-top-color"),
            "press is visible while pointer remains over a controls button");
      view.pointer_move(1, 1);
      view.pointer_button(false);
      CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_NONE,
            "dragging out and releasing does not activate a controls button");
   }

   view.screens.show_screen("pause");
   view.clear_intents();
   view.pointer_button(true);
   view.pointer_leave();
   click_id("quit");
   CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_QUIT,
         "pointer down/up stay in sync after leave");

   view.clear_intents();
   view.screens.show_screen("controls");
   CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_NONE,
         "screen transition drops stale mailbox intents");

   if (argc > 2)
   {
      view.screens.show_screen("pause");
      /* Written through the file layer the menu reads it with. */
      static const char first[] = "first";
      static const char updated[] = "updated image content";
      CHECK(filestream_write_file(argv[2], first, (int64_t)sizeof first - 1),
            "writable thumbnail fixture");
      view.slots.set_slot_state(2, true, argv[2]);
      view.render(960, 600);
      const unsigned before = inspect.texture_loads();
      filestream_write_file(argv[2], updated, (int64_t)sizeof updated - 1);
      view.slots.set_slot_state(2, true, argv[2]);
      view.render(960, 600);
      CHECK(inspect.texture_loads() > before,
            "overwriting a thumbnail reloads the same file without reopening the menu");
   }

   /* We compose the controls scene into menu.rml in the builder, and in the
    * bridge we draw what we receive. We write the remap behind the host
    * boundary, which is not linked here, so this does not prove that a choice
    * is saved or applied. */
   view.screens.show_screen("controls");
   {
      int image_x = 0;
      int image_y = 0;
      const std::string scene(inspect.text("controller-scene"));
      CHECK(scene.find("control-") != std::string::npos,
            "the composed menu carries the generated control callouts");
      CHECK(view.document.element_center("controller-image", &image_x, &image_y),
            "the composed menu carries the controller illustration");
      view.controls.wire(fixture_controls);
      view.controls.set_device_picker(fixture_controls, true, "megadrive6");
      CHECK(std::string(inspect.text("controller-scene")) == scene,
            "naming another pad does not redraw the controls scene");
      CHECK(view.document.has_element("controls-device-option-megadrive6"),
            "picker options are composed markup the bridge only shows");
   }

   /* Every button on the pause row can take focus, and only one at a time.
    *
    * We read the row from the document, not from a fixed table of element
    * ids. With Options enabled the fourth button is `options`, and moving
    * right from SAVE, reaching Options by keyboard or pad, and pressing down
    * from slot 6 must each focus an element. A button added in a design or an
    * export is reachable without any change here. */
   view.screens.show_screen("pause");
   {
      const std::vector<std::string> row = stops_in("pause-panel");
      const size_t count = row.size();

      CHECK(count >= 4, "the pause row has the buttons the design drew");
      CHECK(std::find(row.begin(), row.end(), "options") != row.end(),
            "Options is one of the buttons on the pause row");

      for (size_t index = 0; index < count; ++index)
      {
         view.focus.set(row[index].c_str());
         CHECK(inspect.has_class(row[index].c_str(), "focused"),
               "every button on the pause row can be focused");
         for (size_t other = 0; other < count; ++other)
            if (other != index)
               CHECK(!inspect.has_class(row[other].c_str(), "focused"),
                     "and only one of them at a time");
      }
      CHECK(count && view.focus.current_id() == row[count - 1],
            "the row remembers which button has it");
   }

   view.screens.show_screen("options");
   {
      int slider_x = 0;
      int slider_y = 0;
      int mute_x = 0;
      int mute_y = 0;
      CHECK(view.document.element_center("volume-level", &slider_x, &slider_y),
            "Options has the design's slider");
      CHECK(!view.document.element_center("volume-mute", &mute_x, &mute_y),
            "there is no mute button");
      view.clear_intents();
      view.pointer_move(slider_x, slider_y);
      view.pointer_button(true);
      view.pointer_move(0, 0);
      view.pointer_button(false);
      const auto drag = view.intents.take();
      CHECK(drag.kind == RIB_RMLUI_ACTION_SLIDER,
            "dragging off a slider still sets the level");
      CHECK(drag.id == "volume-level",
            "the slider reports which part moved");
      CHECK(drag.fraction == 0.0f,
            "a drag off the left end is the bottom of the range");

      view.parts.set_slider("volume-level", 0.5f, nullptr);
      view.clear_intents();
      CHECK(!view.parts.nudge_slider("volume-level", 1),
            "a slider with no step does not move, so a key cannot invent one");
      view.parts.set_slider_step("volume-level", 0.1f);
      CHECK(view.parts.nudge_slider("volume-level", 1), "a key nudges the focused slider");
      const auto nudge = view.intents.take();
      CHECK(nudge.kind == RIB_RMLUI_ACTION_SLIDER,
            "the nudge is the same change a drag commits");
      CHECK(nudge.fraction > 0.59f && nudge.fraction < 0.61f,
            "the nudge adds the slider's own step, not a volume-shaped one");

      view.parts.set_slider("volume-level", 1.0f, nullptr);
      view.parts.set_slider_step("volume-level", 0.125f);
      view.clear_intents();
      click_id("volume-down");
      const auto arrow = view.intents.take();
      CHECK(arrow.kind == RIB_RMLUI_ACTION_SLIDER,
            "the left arrow is the slider moving down one position");
      CHECK(arrow.fraction > 0.87f && arrow.fraction < 0.88f,
            "one arrow is one step of the slider");

      /* With Options open and the slider selected, Down skips the left and
       * right arrows of the slider. Left and Right on the slider already step
       * it, so those arrows are for the pointer and are not stops. Down moves
       * to the next focusable element, as for the player. */
      {
         const std::vector<std::string> ids = stops_in("options-panel");
         const int count = (int)ids.size();
         int slider = -1;
         for (int index = 0; index < count; ++index)
            if (ids[index] == "volume-level")
               slider = index;
         CHECK(slider >= 0, "the volume slider is a focus stop");
         const char *landed = (slider >= 0 && slider + 1 < count)
               ? ids[slider + 1].c_str() : "";
         const char *again = (slider >= 0 && slider + 2 < count)
               ? ids[slider + 2].c_str() : "";
         char message[192];
         std::snprintf(message, sizeof(message),
               "pressing down from the slider lands on %s", landed);
         CHECK(std::strcmp(landed, "controls") == 0, message);
         std::snprintf(message, sizeof(message),
               "pressing down again lands on %s", again);
         CHECK(std::strcmp(again, "volume-up") != 0, message);
         int arrow_x = 0;
         int arrow_y = 0;
         CHECK(view.document.element_center("volume-down", &arrow_x, &arrow_y),
               "the left arrow is still there for a pointer");
         CHECK(view.document.element_center("volume-up", &arrow_x, &arrow_y),
               "the right arrow is still there for a pointer");
      }

      /* DISC is in the document for every game, because we cannot create it
       * in the player once the core has reported how many images it loaded.
       * While that count is one or none we hide it, and a hidden entry must
       * not take focus, or Down moves to it and no ring is drawn. Showing it
       * must not move the entries above it. */
      {
         std::vector<std::string> ids = stops_in("options-panel");
         bool landed = std::find(ids.begin(), ids.end(), "discs") != ids.end();
         int controls_y = 0;
         int controls_x = 0;
         CHECK(!landed, "a hidden disc entry is not a focus stop");
         CHECK(view.document.element_center("controls", &controls_x, &controls_y),
               "controls is where it was");
         const int before = controls_y;
         view.document.set_shown("discs", true);
         view.document.set_disabled("discs", false);
         CHECK(view.document.element_center("controls", &controls_x, &controls_y),
               "controls is still there once disc is shown");
         CHECK(controls_y == before,
               "showing the disc entry does not move the entries above it");
         view.document.set_shown("discs", false);
         view.document.set_disabled("discs", true);
         ids = stops_in("options-panel");
         landed = std::find(ids.begin(), ids.end(), "discs") != ids.end();
         CHECK(!landed, "hiding the disc entry takes it back out of the walk");
      }
      {
         /* Long enough that we still have to shorten it in the one-line column.
          * The picture column is narrower, so we shorten the same string sooner
          * there. The one-line row uses the width that the picture left free. */
         const char *long_name =
            "/Volumes/Library/Games/Final Fantasy VII/Final Fantasy VII/"
            "Final Fantasy VII/Final Fantasy VII (USA) (Disc 4).cue";
         view.lists.fit_row_title("fixture-one", long_name);
         const std::string fitted(inspect.text("fixture-one-title"));
         char message[512];
         std::snprintf(message, sizeof(message),
               "a long disc name is shortened in the middle, got '%s'",
               fitted.c_str());
         const auto dots = fitted.find("\u2026");
         const auto number = fitted.rfind("(Disc 4)");
         CHECK(dots != std::string::npos, message);
         std::snprintf(message, sizeof(message),
               "the disc number stays after the ellipsis, got '%s'",
               fitted.c_str());
         CHECK(number != std::string::npos && dots < number, message);
         CHECK(fitted.size() < std::strlen(long_name),
               "the shortened name is shorter than the path");
         view.document.set_shown("fixture-panel", true);
         view.lists.fit_row_title("fixture-two", long_name);
         const std::string pictured(inspect.text("fixture-two-title"));
         const auto picture_dots = pictured.find("\u2026");
         std::snprintf(message, sizeof(message),
               "the one-line row still shortens where the picture column does, line '%s' picture '%s'",
               fitted.c_str(), pictured.c_str());
         CHECK(dots != std::string::npos && picture_dots != std::string::npos
               && dots > picture_dots, message);
         int row_x = 0, row_y = 0, row_w = 0, row_h = 0;
         int title_x = 0, title_y = 0, title_w = 0, title_h = 0;
         int state_x = 0, state_y = 0, state_w = 0, state_h = 0;
         int pic_x = 0, pic_y = 0, pic_w = 0, pic_h = 0;
         int pic_title_x = 0, pic_title_y = 0, pic_title_w = 0, pic_title_h = 0;
         CHECK(view.document.element_box("fixture-one", &row_x, &row_y, &row_w, &row_h),
               "the one-line row has a box");
         CHECK(view.document.element_box("fixture-one-title", &title_x, &title_y, &title_w, &title_h),
               "the one-line name has a box");
         CHECK(view.document.element_box("fixture-one-state", &state_x, &state_y, &state_w, &state_h),
               "the one-line state has a box");
         CHECK(view.document.element_box("fixture-two", &pic_x, &pic_y, &pic_w, &pic_h),
               "the picture row has a box");
         CHECK(view.document.element_box("fixture-two-title",
               &pic_title_x, &pic_title_y, &pic_title_w, &pic_title_h),
               "the picture row's name has a box");
         std::snprintf(message, sizeof(message),
               "a row with no second line is still %d tall, the picture row is %d",
               row_h, pic_h);
         CHECK(row_h > 0 && row_h < pic_h * 3 / 4, message);
         std::snprintf(message, sizeof(message),
               "the name sits in the top half (title y %d h %d, row y %d h %d)",
               title_y, title_h, row_y, row_h);
         CHECK(std::abs((title_y + title_h / 2) - (row_y + row_h / 2)) <= 2, message);
         std::snprintf(message, sizeof(message),
               "IN is not on the name's line (state y %d h %d, title y %d h %d)",
               state_y, state_h, title_y, title_h);
         CHECK(std::abs((state_y + state_h / 2) - (title_y + title_h / 2)) <= 2, message);
         std::snprintf(message, sizeof(message),
               "the name still starts in the picture column (title x %d, picture title x %d, row x %d)",
               title_x, pic_title_x, row_x);
         CHECK(title_x - row_x + 8 < pic_title_x - pic_x, message);
         check_focus_leaves_the_name();
         view.document.set_shown("fixture-panel", false);
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
      view.screens.show_screen("pause");
      drain_actions();
      press_then_release_at("save", 4, 4);
      CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_NONE,
            "pressing a button and releasing off it does nothing");

      // The other half, to show that this does not pass because clicks have
      // stopped working: a press and release on the same button still acts.
      drain_actions();
      click_id("save");
      CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_SAVE,
            "pressing and releasing on a button still presses it");

      // Sliding off and back on is a press, because the release happens on the
      // element where the press began.
      drain_actions();
      {
         int x = 0;
         int y = 0;
         CHECK(view.document.element_center("save", &x, &y), "element has a hit centre");
         view.pointer_move(x, y);
         view.pointer_button(true);
         view.pointer_move(4, 4);
         view.pointer_move(x, y);
         view.pointer_button(false);
      }
      CHECK(view.intents.take().kind == RIB_RMLUI_ACTION_SAVE,
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
         RIB_RMLUI_ACTION_SELECT_SLOT,
      };
      for (int action = RIB_RMLUI_ACTION_NONE;
            action <= RIB_RMLUI_ACTION_SHOW_SCREEN; ++action)
      {
         bool expected_silent = false;
         for (int quiet : silent)
            if (quiet == action)
               expected_silent = true;
         const bool is_silent =
            rib::action_sound(action) == rib::Sound::None;
         CHECK(is_silent == expected_silent,
               "every intent is audible unless silence was chosen for it");
      }
      CHECK(rib::action_sound(RIB_RMLUI_ACTION_SHOW_SCREEN)
            == rib::Sound::Confirm, "changing screen is confirmed, not silent");
      CHECK(rib::action_sound(RIB_RMLUI_ACTION_CONTROLS_BACK)
            == rib::Sound::Cancel, "leaving a screen cancels, not confirms");
   }

   // We draw an overlay over a running game, and how it looks is up to the
   // design. In the player we only move an element between three states and
   // state whether the menu is on screen. If the rules in the design did not
   // act on that, the notice would appear and vanish without the arriving,
   // leaving or hiding animations in the design.
   {
      rib::paint_overlay(view.document, "notice", rib::RIB_OVERLAY_HIDDEN);
      view.set_overlay_mode(true);
      view.render(960, 600);
      const std::string away = inspect.property("notice", "opacity");
      const std::string resting = inspect.property("notice", "bottom");
      CHECK(std::string(inspect.property("footer", "display")) == "none",
            "the design puts the menu away while only overlays are drawn");

      rib::paint_overlay(view.document, "notice", rib::RIB_OVERLAY_SHOWING);
      settle(0.6);
      const std::string shown = inspect.property("notice", "opacity");
      CHECK(shown != away, "showing an overlay makes the design draw it");
      CHECK(std::string(inspect.property("notice", "bottom")) != resting,
            "the design moves the notice into place, not only fades it in");

      rib::paint_overlay(view.document, "notice", rib::RIB_OVERLAY_LEAVING);
      view.render(960, 600);
      CHECK(std::string(inspect.property("notice", "opacity")) != away,
            "leaving is a transition, not a cut: the first frame is still drawn");
      settle(1.0);
      CHECK(std::string(inspect.property("notice", "opacity")) == away,
            "the design takes the notice away over its own declared time");

      rib::paint_overlay(view.document, "notice", rib::RIB_OVERLAY_HIDDEN);
      view.set_overlay_mode(false);
      view.render(960, 600);
      CHECK(std::string(inspect.property("footer", "display")) != "none",
            "the menu comes back when it is what is on screen");
   }

   // The BACK of a list screen comes after its rows, so moving down with the
   // keyboard past the last row reaches it.
   {
      view.screens.declare_screen(screen("fixture", "fixture-panel", "LIST", "ESC  BACK", ""));
      CHECK(view.screens.show_screen("fixture"), "a declared list screen shows");
      view.lists.wire_lists();
      drain_actions();
      const std::vector<std::string> stops = stops_in("fixture-panel");
      CHECK(stops.size() == 5 && stops.back() == "fixture-back",
            "the list's rows are stops, and its BACK after them");
      drain_actions();
   }

   // Each queued slider input must keep its payload until we consume it.
   view.screens.show_screen("options");
   view.clear_intents();
   CHECK(view.parts.commit_slider("volume-level", 0.25f), "first queued slider input");
   CHECK(view.parts.commit_slider("volume-level", 0.75f), "second queued slider input");
   const auto queued_first = view.intents.take();
   CHECK(queued_first.kind == RIB_RMLUI_ACTION_SLIDER, "first queued slider kind");
   CHECK(queued_first.fraction == 0.25f, "first queued slider keeps its own value");
   const auto queued_second = view.intents.take();
   CHECK(queued_second.kind == RIB_RMLUI_ACTION_SLIDER, "second queued slider kind");
   CHECK(queued_second.fraction == 0.75f, "second queued slider keeps its own value");

   // When we create the document again, slider values and configured steps stay.
   view.parts.set_slider_step("volume-level", 0.125f);
   view.shutdown();
   CHECK(view.initialize(argv[1], kFonts, 960, 600, false, fixture_controls), "document recreates for slider state");
   view.clear_intents();
   CHECK(view.parts.nudge_slider("volume-level", -1), "recreated slider retains its step");
   const auto recreated_slider = view.intents.take();
   CHECK(recreated_slider.kind == RIB_RMLUI_ACTION_SLIDER
         && recreated_slider.fraction == 0.625f,
         "recreated slider steps from its previous value");
   view.shutdown();
   if (failures)
   {
      std::fprintf(stderr, "%d check(s) failed\n", failures);
      return 1;
   }
   std::printf("ok\n");
   return 0;
}
