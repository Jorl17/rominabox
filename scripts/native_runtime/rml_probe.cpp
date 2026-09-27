/* Drive the menu's RML document with synthetic input and print the result.
 *
 * We load the same menu.rml and menu.rcss as in an exported game, send them
 * mouse and keyboard events, and print the resulting element classes as JSON
 * Lines, so that we can test hover, press and focus behaviour. That includes
 * the check `if (hovered_action == action)` in the bridge's HoverListener
 * against a Mouseout that arrives after the next Mouseover. We render nothing
 * (no OpenGL, no window, no RetroArch), and test only the RmlUi layer.
 *
 *   rml_probe --document DOC [--size WxH] [--step SPEC ...] [--steps FILE]
 *
 *     --step move:X,Y          move the pointer
 *     --step down:X,Y          press the left button at a point
 *     --step up:X,Y            release it
 *     --step click:X,Y         move, press and release
 *     --step key:NAME          press and release a key (up, down, left,
 *                              right, return, escape, tab)
 *     --step watch:ID          print element ID's state after every later step
 *     --step box:ID            print element ID's border box after every later step
 *     --steps FILE             more steps, one SPEC to a line, for more than fits
 *                              on a command line (Windows allows 32,767 characters)
 *
 * Coordinates are document pixels. For each step we print one JSON object with
 * the step, the element under the pointer and the classes of every watched
 * element, so that a test can check values instead of a screenshot.
 */

#include <RmlUi/Core.h>

#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

namespace {

/* RmlUi cannot run without a system interface. It must not write to stdout,
 * because stdout is the machine-readable output of this tool. */
class QuietSystem : public Rml::SystemInterface {
public:
    double GetElapsedTime() override { return elapsed; }
    bool LogMessage(Rml::Log::Type type, const Rml::String& message) override
    {
        if (type == Rml::Log::LT_ERROR || type == Rml::Log::LT_ASSERT)
            std::fprintf(stderr, "rmlui: %s\n", message.c_str());
        return true;
    }
    /* We advance this ourselves, so animations and transitions do not depend
     * on how fast the machine ran the test. */
    double elapsed = 0.0;
};

/* A render interface that draws nothing. Layout, hit testing and event
 * dispatch all work without it, and they are what we test. */
class NoRenderer : public Rml::RenderInterface {
public:
    Rml::CompiledGeometryHandle CompileGeometry(Rml::Span<const Rml::Vertex>,
                                                Rml::Span<const int>) override
    {
        return 1;
    }
    void ReleaseGeometry(Rml::CompiledGeometryHandle) override {}
    void RenderGeometry(Rml::CompiledGeometryHandle, Rml::Vector2f,
                        Rml::TextureHandle) override
    {
    }
    Rml::TextureHandle LoadTexture(Rml::Vector2i& dimensions,
                                   const Rml::String&) override
    {
        dimensions = {1, 1};
        return 1;
    }
    Rml::TextureHandle GenerateTexture(Rml::Span<const Rml::byte>,
                                       Rml::Vector2i) override
    {
        return 1;
    }
    void ReleaseTexture(Rml::TextureHandle) override {}
    void EnableScissorRegion(bool) override {}
    void SetScissorRegion(Rml::Rectanglei) override {}
};

std::string escape(const std::string& value)
{
    std::string out;
    for (char character : value) {
        if (character == '"' || character == '\\') out += '\\';
        out += character;
    }
    return out;
}

/* The classes that RmlUi reports. A design styles elements by this state, so
 * this is the state that we check. */
std::string classes_of(Rml::Element* element)
{
    if (!element) return "";
    return std::string(element->GetClassNames());
}

Rml::Input::KeyIdentifier key_named(const std::string& name)
{
    if (name == "up") return Rml::Input::KI_UP;
    if (name == "down") return Rml::Input::KI_DOWN;
    if (name == "left") return Rml::Input::KI_LEFT;
    if (name == "right") return Rml::Input::KI_RIGHT;
    if (name == "return" || name == "enter") return Rml::Input::KI_RETURN;
    if (name == "escape") return Rml::Input::KI_ESCAPE;
    if (name == "tab") return Rml::Input::KI_TAB;
    return Rml::Input::KI_UNKNOWN;
}

struct Step {
    std::string verb;
    std::string argument;
    int x = 0;
    int y = 0;
};

bool parse_point(const std::string& text, int& x, int& y)
{
    const auto comma = text.find(',');
    if (comma == std::string::npos) return false;
    x = std::stoi(text.substr(0, comma));
    y = std::stoi(text.substr(comma + 1));
    return true;
}

} // namespace

/* One step as written on the command line or in a steps file, VERB:ARGUMENT. */
bool add_step(const std::string& spec, std::vector<Step>& steps)
{
    const auto colon = spec.find(':');
    if (colon == std::string::npos) {
        std::fprintf(stderr, "--step wants VERB:ARGUMENT\n");
        return false;
    }
    Step step{spec.substr(0, colon), spec.substr(colon + 1), 0, 0};
    if (step.verb != "key" && step.verb != "watch" && step.verb != "box" &&
        !parse_point(step.argument, step.x, step.y)) {
        std::fprintf(stderr, "step '%s' wants X,Y\n", spec.c_str());
        return false;
    }
    steps.push_back(step);
    return true;
}

int main(int argc, char** argv)
{
    std::string document_path;
    int width = 960;
    int height = 600;
    std::vector<Step> steps;

    for (int i = 1; i < argc; i++) {
        const std::string argument = argv[i];
        if (argument == "--document" && i + 1 < argc) {
            document_path = argv[++i];
        } else if (argument == "--size" && i + 1 < argc) {
            const std::string value = argv[++i];
            const auto x = value.find('x');
            if (x == std::string::npos) {
                std::fprintf(stderr, "--size wants WxH\n");
                return 2;
            }
            width = std::stoi(value.substr(0, x));
            height = std::stoi(value.substr(x + 1));
        } else if (argument == "--step" && i + 1 < argc) {
            if (!add_step(argv[++i], steps))
                return 2;
        } else if (argument == "--steps" && i + 1 < argc) {
            std::ifstream file(argv[++i]);
            if (!file) {
                std::fprintf(stderr, "cannot read the steps in %s\n", argv[i]);
                return 2;
            }
            for (std::string line; std::getline(file, line);) {
                if (!line.empty() && line.back() == '\r')
                    line.pop_back();
                if (!line.empty() && !add_step(line, steps))
                    return 2;
            }
        } else {
            std::fprintf(stderr, "unknown argument: %s\n", argument.c_str());
            return 2;
        }
    }
    if (document_path.empty()) {
        std::fprintf(stderr, "give --document PATH\n");
        return 2;
    }

    NoRenderer renderer;
    QuietSystem system;
    Rml::SetRenderInterface(&renderer);
    Rml::SetSystemInterface(&system);
    if (!Rml::Initialise()) {
        std::fprintf(stderr, "RmlUi would not initialise\n");
        return 3;
    }

    const std::filesystem::path document_file(document_path);
    const auto font = document_file.parent_path() / "Silkscreen-Regular.ttf";
    /* Without a font, text elements collapse to zero size and hit testing
     * no longer matches them, so the output of this tool would be wrong
     * while looking plausible. */
    if (!Rml::LoadFontFace(font.string())) {
        std::fprintf(stderr, "no font at %s\n", font.string().c_str());
        return 4;
    }

    Rml::Context* context = Rml::CreateContext("probe", {width, height});
    if (!context) {
        std::fprintf(stderr, "no RmlUi context\n");
        return 3;
    }
    context->SetDensityIndependentPixelRatio(1.0f);

    Rml::ElementDocument* document = context->LoadDocument(document_path);
    if (!document) {
        std::fprintf(stderr, "could not load %s\n", document_path.c_str());
        return 5;
    }
    document->Show();
    context->Update();

    std::vector<std::string> watched;
    std::vector<std::string> boxed;
    int pointer_x = -1;
    int pointer_y = -1;

    for (const Step& step : steps) {
        if (step.verb == "watch") {
            watched.push_back(step.argument);
            continue;
        }
        if (step.verb == "box") {
            boxed.push_back(step.argument);
            continue;
        }
        if (step.verb == "move") {
            context->ProcessMouseMove(step.x, step.y, 0);
            pointer_x = step.x;
            pointer_y = step.y;
        } else if (step.verb == "down") {
            context->ProcessMouseMove(step.x, step.y, 0);
            context->ProcessMouseButtonDown(0, 0);
            pointer_x = step.x;
            pointer_y = step.y;
        } else if (step.verb == "up") {
            context->ProcessMouseButtonUp(0, 0);
        } else if (step.verb == "click") {
            context->ProcessMouseMove(step.x, step.y, 0);
            context->ProcessMouseButtonDown(0, 0);
            context->ProcessMouseButtonUp(0, 0);
            pointer_x = step.x;
            pointer_y = step.y;
        } else if (step.verb == "key") {
            const auto key = key_named(step.argument);
            if (key == Rml::Input::KI_UNKNOWN) {
                std::fprintf(stderr, "unknown key: %s\n", step.argument.c_str());
                return 2;
            }
            context->ProcessKeyDown(key, 0);
            context->ProcessKeyUp(key, 0);
        } else {
            std::fprintf(stderr, "unknown step verb: %s\n", step.verb.c_str());
            return 2;
        }

        /* We advance the clock by a whole frame per step, so that a transition
         * cannot leave an element halfway and give different classes for the
         * same script on a slower machine. */
        system.elapsed += 1.0 / 60.0;
        context->Update();

        Rml::Element* under = nullptr;
        if (pointer_x >= 0)
            under = context->GetHoverElement();

        std::printf("{\"step\":\"%s:%s\",\"hover\":\"%s\",\"watched\":{",
                    escape(step.verb).c_str(), escape(step.argument).c_str(),
                    under ? escape(std::string(under->GetId())).c_str() : "");
        for (size_t i = 0; i < watched.size(); i++) {
            Rml::Element* element = document->GetElementById(watched[i]);
            std::printf("%s\"%s\":{\"present\":%s,\"classes\":\"%s\"}",
                        i ? "," : "", escape(watched[i]).c_str(),
                        element ? "true" : "false",
                        element ? escape(classes_of(element)).c_str() : "");
        }
        std::printf("}");
        /* Where layout put each element: x, y, width, height in document
         * pixels, or null when there is no such element. */
        if (!boxed.empty()) {
            std::printf(",\"boxes\":{");
            for (size_t i = 0; i < boxed.size(); i++) {
                Rml::Element* element = document->GetElementById(boxed[i]);
                std::printf("%s\"%s\":", i ? "," : "", escape(boxed[i]).c_str());
                if (!element) {
                    std::printf("null");
                    continue;
                }
                const Rml::Vector2f at = element->GetAbsoluteOffset(Rml::BoxArea::Border);
                const Rml::Vector2f size = element->GetBox().GetSize(Rml::BoxArea::Border);
                std::printf("[%g,%g,%g,%g]", at.x, at.y, size.x, size.y);
            }
            std::printf("}");
        }
        std::printf("}\n");
    }

    context->UnloadDocument(document);
    Rml::Shutdown();
    return 0;
}
