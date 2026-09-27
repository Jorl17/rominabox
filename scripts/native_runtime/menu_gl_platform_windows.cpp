/* The context of the menu GL probe on Windows: WGL on a window that is never
 * shown. In the menu renderer we call OpenGL through RetroArch's loader. In
 * the player we resolve that loader in the WGL driver once its context is
 * current, first with wglGetProcAddress and then from the exports of
 * opengl32.dll (gfx/drivers_context/wgl_ctx.c), and here we do the same. */

#include "menu_gl_platform.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <GL/gl.h>
#include <GL/wglext.h>
#include <glsym/rglgen.h>
#include <shellapi.h>

struct ProbeContext
{
   HWND window;
   HDC device;
   HGLRC gl;
};

static const wchar_t window_class[] = L"RomInABoxMenuGlProbe";

/* `argv` is in the ANSI code page, and only the wide command line is exact. */
static std::vector<std::string> utf8_arguments()
{
   std::vector<std::string> arguments;
   int count = 0;
   wchar_t **wide = CommandLineToArgvW(GetCommandLineW(), &count);
   for (int i = 0; wide && i < count; i++)
   {
      const int length = (int)wcslen(wide[i]);
      const int size = WideCharToMultiByte(CP_UTF8, 0, wide[i], length, nullptr, 0, nullptr, nullptr);
      std::string argument(size > 0 ? size : 0, ' ');
      if (size > 0)
         WideCharToMultiByte(CP_UTF8, 0, wide[i], length, &argument[0], size, nullptr, nullptr);
      arguments.push_back(argument);
   }
   LocalFree(wide);
   return arguments;
}

std::vector<std::string> probe_platform_start(int, char **)
{
   WNDCLASSW description = {};
   description.style = CS_OWNDC;
   description.lpfnWndProc = DefWindowProcW;
   description.hInstance = GetModuleHandleW(nullptr);
   description.lpszClassName = window_class;
   RegisterClassW(&description);
   return utf8_arguments();
}

static rglgen_func_t proc_address(const char *name)
{
   PROC found = wglGetProcAddress(name);
   if (!found)
      found = GetProcAddress(GetModuleHandleW(L"opengl32.dll"), name);
   return reinterpret_cast<rglgen_func_t>(found);
}

static void release(ProbeContext *context)
{
   wglMakeCurrent(nullptr, nullptr);
   if (context->gl)
      wglDeleteContext(context->gl);
   if (context->device)
      ReleaseDC(context->window, context->device);
   DestroyWindow(context->window);
   delete context;
}

ProbeContext *probe_context_create(bool core)
{
   /* Never given WS_VISIBLE and never shown. */
   HWND window = CreateWindowExW(0, window_class, L"", WS_POPUP, 0, 0, 64, 64,
         nullptr, nullptr, GetModuleHandleW(nullptr), nullptr);
   if (!window)
      return nullptr;
   ProbeContext *context = new ProbeContext{window, GetDC(window), nullptr};

   PIXELFORMATDESCRIPTOR format = {};
   format.nSize = sizeof format;
   format.nVersion = 1;
   format.dwFlags = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER;
   format.iPixelType = PFD_TYPE_RGBA;
   format.cColorBits = 24;
   format.cAlphaBits = 8;
   format.cStencilBits = 8;
   const int chosen = ChoosePixelFormat(context->device, &format);
   if (!chosen || !SetPixelFormat(context->device, chosen, &format))
   {
      release(context);
      return nullptr;
   }

   /* A legacy context first: wglCreateContextAttribsARB is only found
    * through a current one. */
   context->gl = wglCreateContext(context->device);
   if (!context->gl || !wglMakeCurrent(context->device, context->gl))
   {
      release(context);
      return nullptr;
   }
   if (core)
   {
      auto create = reinterpret_cast<PFNWGLCREATECONTEXTATTRIBSARBPROC>(
            proc_address("wglCreateContextAttribsARB"));
      const int attributes[] = {
         WGL_CONTEXT_MAJOR_VERSION_ARB, 3,
         WGL_CONTEXT_MINOR_VERSION_ARB, 2,
         WGL_CONTEXT_PROFILE_MASK_ARB, WGL_CONTEXT_CORE_PROFILE_BIT_ARB,
         0
      };
      HGLRC profile = create ? create(context->device, nullptr, attributes) : nullptr;
      wglMakeCurrent(nullptr, nullptr);
      wglDeleteContext(context->gl);
      context->gl = profile;
      if (!profile || !wglMakeCurrent(context->device, profile))
      {
         release(context);
         return nullptr;
      }
   }
   rglgen_resolve_symbols(proc_address);
   return context;
}

void probe_context_release(ProbeContext *context)
{
   release(context);
}
