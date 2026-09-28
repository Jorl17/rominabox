#pragma once
#include "rmlui/document.hpp"
#include "rmlui/elements.hpp"
#include <RmlUi/Core/ElementUtilities.h>
#include <cmath>
#include <vector>
namespace rib::test {
struct Inspection
{
   explicit Inspection(Document& document) : document(document) {}
   Document& document;

bool row_glyphs_overlap(const char *row_id)
{
   if (!document.root() || !document.get_context() || !row_id)
      return false;
   document.get_context()->Update();
   Rml::Element *row = document.root()->GetElementById(row_id);
   if (!row || hidden(row))
      return false;
   auto span = [](Rml::Element *element, bool right_aligned,
         float &left, float &right) -> bool {
      if (!element || display_none(element))
         return false;
      const Rml::String text = element->GetInnerRML();
      if (text.empty())
         return false;
      const float width = (float)Rml::ElementUtilities::GetStringWidth(element, text);
      if (width <= 0.f)
         return false;
      const Rml::Vector2f at = element->GetAbsoluteOffset(Rml::BoxArea::Padding);
      const float box = element->GetBox().GetSize(Rml::BoxArea::Content).x;
      if (right_aligned)
      {
         right = at.x + box;
         left = right - width;
      }
      else
      {
         left = at.x;
         right = left + width;
      }
      return true;
   };
   std::vector<Rml::Element*> titles;
   std::vector<Rml::Element*> details;
   collect(row, "list-row-title", titles);
   collect(row, "list-row-detail", details);
   if (titles.empty() || details.empty())
      return false;
   float title_left = 0.f, title_right = 0.f, detail_left = 0.f, detail_right = 0.f;
   if (!span(titles[0], false, title_left, title_right)
         || !span(details[0], true, detail_left, detail_right))
      return false;
   return title_right > detail_left + 0.5f;
}

bool box(const char *id, int *x, int *y, int *w, int *h)
{
   if (!document.root() || !document.get_context() || !id || !x || !y || !w || !h)
      return false;
   document.get_context()->Update();
   Rml::Element *element = document.root()->GetElementById(id);
   if (!element || hidden(element))
      return false;
   const Rml::Vector2f at = element->GetAbsoluteOffset(Rml::BoxArea::Border);
   const Rml::Vector2f size = element->GetBox().GetSize(Rml::BoxArea::Border);
   if (size.x <= 0.f || size.y <= 0.f)
      return false;
   *x = (int)std::floor(at.x);
   *y = (int)std::floor(at.y);
   *w = (int)std::ceil(at.x + size.x) - *x;
   *h = (int)std::ceil(at.y + size.y) - *y;
   return true;
}

int class_count(const char *class_name)
{
   std::vector<Rml::Element*> found;
   int count = 0;
   collect(document.root(), class_name, found);
   for (Rml::Element *element : found)
      if (!element->GetId().empty() && !hidden(element))
         ++count;
   return count;
}

const char *class_id(const char *class_name, int index)
{
   static std::string id;
   std::vector<Rml::Element*> found;
   int seen = 0;
   id.clear();
   collect(document.root(), class_name, found);
   for (Rml::Element *element : found)
   {
      if (element->GetId().empty() || hidden(element))
         continue;
      if (seen == index)
      {
         id = element->GetId();
         return id.c_str();
      }
      ++seen;
   }
   return "";
}

/* A value, instead of a pointer into a buffer rewritten on each call,
 * because a comparison of two properties would read that buffer twice, and
 * the order of the two calls is up to the compiler. */
std::string property(const char *id, const char *property)
{
   document.get_context()->Update();
   auto *element = document.root()->GetElementById(id);
   return element && element->GetProperty(property) ? element->GetProperty(property)->ToString() : "";
}

unsigned texture_loads() { return document.texture_loads(); }

void advance(double seconds) { document.advance(seconds); }

std::string text(const char *id) {
   Rml::Element *element = document.root() ? document.root()->GetElementById(id) : nullptr;
   return element ? element->GetInnerRML() : std::string("<no element ") + id + ">";
}

/* The words in an element, without the markup used for styling in a design. */
std::string words(const char *id) {
   Rml::Element *element = document.root() ? document.root()->GetElementById(id) : nullptr;
   return element ? rib::words_of(element) : std::string("<no element ") + id + ">";
}

bool has_class(const char *id, const char *name) {
   Rml::Element *element = document.root() && id ? document.root()->GetElementById(id) : nullptr;
   return element && name && element->IsClassSet(name);
}

};
}
