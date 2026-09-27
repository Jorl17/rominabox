/* What a fullscreen Windows game shows when we present through DXGI
 * (gfx/drivers_context/wgl_dxgi.c in the fork). We draw a picture with OpenGL,
 * a red band along the top over blue, into the presenter framebuffer and
 * present it, and the Windows capture of the window must have the band at the
 * top. We check again after the window changes size, where a picture of the
 * old size stretched over the window would make the band taller.
 *
 * The window is beyond the desktop and never takes focus, so nothing appears
 * on anyone's screen. The capture is the full content from PrintWindow, which
 * is what Windows composes for the window, the swapchain included. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <GL/gl.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

#include "gfx/drivers_context/wgl_dxgi.h"

#ifndef PW_RENDERFULLCONTENT
#define PW_RENDERFULLCONTENT 0x00000002
#endif
#define FRAMEBUFFER 0x8D40

/* The fork's log lines, printed. */
void RARCH_LOG(const char *format, ...)
{
   va_list arguments;
   va_start(arguments, format);
   vfprintf(stderr, format, arguments);
   va_end(arguments);
}
void RARCH_WARN(const char *format, ...)
{
   va_list arguments;
   va_start(arguments, format);
   vfprintf(stderr, format, arguments);
   va_end(arguments);
}
void RARCH_ERR(const char *format, ...)
{
   va_list arguments;
   va_start(arguments, format);
   vfprintf(stderr, format, arguments);
   va_end(arguments);
}

static int failures;

static void check(int condition, const char *what)
{
   if (!condition)
   {
      fprintf(stderr, "FAIL: %s\n", what);
      failures++;
   }
}

static void pump(void)
{
   MSG message;
   while (PeekMessageW(&message, NULL, 0, 0, PM_REMOVE))
   {
      TranslateMessage(&message);
      DispatchMessageW(&message);
   }
}

/* A red band BAND rows tall along the top, blue below it. The band's height
 * is fixed, so a picture of the old size stretched over the new one shows it
 * elsewhere. */
#define BAND 8

/* Blue, then the band, in OpenGL's rows (bottom up). */
static void draw(wgl_dxgi_t *dxgi, int width, int height)
{
   void (APIENTRY *bind_framebuffer)(GLenum, GLuint) =
         (void (APIENTRY *)(GLenum, GLuint))(void*)wglGetProcAddress("glBindFramebuffer");
   bind_framebuffer(FRAMEBUFFER, wgl_dxgi_framebuffer(dxgi));
   glViewport(0, 0, width, height);
   glClearColor(0.0f, 0.0f, 1.0f, 1.0f);
   glClear(GL_COLOR_BUFFER_BIT);
   glEnable(GL_SCISSOR_TEST);
   glScissor(0, height - BAND, width, BAND);
   glClearColor(1.0f, 0.0f, 0.0f, 1.0f);
   glClear(GL_COLOR_BUFFER_BIT);
   glDisable(GL_SCISSOR_TEST);
}

/* The window as Windows composes it: `top` gets the pixel in the middle of
 * the band, `bottom` the one just below it, down the middle, as 0xRRGGBB. */
static int capture(HWND window, int width, int height, DWORD *top, DWORD *bottom)
{
   BITMAPINFO format;
   void *pixels = NULL;
   HDC screen = GetDC(NULL);
   HDC memory = CreateCompatibleDC(screen);
   HBITMAP bitmap;
   int captured;
   memset(&format, 0, sizeof format);
   format.bmiHeader.biSize = sizeof format.bmiHeader;
   format.bmiHeader.biWidth = width;
   format.bmiHeader.biHeight = -height; /* top down */
   format.bmiHeader.biPlanes = 1;
   format.bmiHeader.biBitCount = 32;
   format.bmiHeader.biCompression = BI_RGB;
   bitmap = CreateDIBSection(memory, &format, DIB_RGB_COLORS, &pixels, NULL, 0);
   SelectObject(memory, bitmap);
   captured = PrintWindow(window, memory, PW_RENDERFULLCONTENT);
   if (captured)
   {
      const DWORD *row = (const DWORD*)pixels;
      *top = row[(BAND / 2) * width + width / 2] & 0xFFFFFF;
      *bottom = row[(BAND + 2) * width + width / 2] & 0xFFFFFF;
   }
   DeleteObject(bitmap);
   DeleteDC(memory);
   ReleaseDC(NULL, screen);
   return captured;
}

static void shows_upright(HWND window, wgl_dxgi_t *dxgi, int width, int height, const char *when)
{
   DWORD top = 0, bottom = 0;
   char what[160];
   int frame;
   for (frame = 0; frame < 3; frame++)
   {
      pump();
      draw(dxgi, width, height);
      wgl_dxgi_present(dxgi, 1);
   }
   Sleep(100);
   pump();
   snprintf(what, sizeof what, "%s: Windows captures the window", when);
   check(capture(window, width, height, &top, &bottom), what);
   printf("%s: top %06lx, bottom %06lx\n", when, (unsigned long)top, (unsigned long)bottom);
   snprintf(what, sizeof what, "%s: the top shows red (got %06lx)", when, (unsigned long)top);
   check(top == 0xFF0000, what);
   snprintf(what, sizeof what, "%s: the bottom shows blue (got %06lx)", when, (unsigned long)bottom);
   check(bottom == 0x0000FF, what);
}

int main(void)
{
   WNDCLASSW kind;
   PIXELFORMATDESCRIPTOR pixel;
   HWND window;
   HDC dc;
   HGLRC context;
   wgl_dxgi_t *dxgi;
   /* Beyond the right edge of every monitor. */
   const int x = GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) + 200;

   memset(&kind, 0, sizeof kind);
   kind.lpfnWndProc = DefWindowProcW;
   kind.hInstance = GetModuleHandleW(NULL);
   kind.lpszClassName = L"test_wgl_dxgi";
   RegisterClassW(&kind);
   window = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, L"test_wgl_dxgi", L"test_wgl_dxgi",
         WS_POPUP, x, 0, 64, 48, NULL, NULL, kind.hInstance, NULL);
   ShowWindow(window, SW_SHOWNOACTIVATE);

   dc = GetDC(window);
   memset(&pixel, 0, sizeof pixel);
   pixel.nSize = sizeof pixel;
   pixel.nVersion = 1;
   pixel.dwFlags = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER;
   pixel.iPixelType = PFD_TYPE_RGBA;
   pixel.cColorBits = 32;
   pixel.cDepthBits = 24;
   pixel.cStencilBits = 8;
   SetPixelFormat(dc, ChoosePixelFormat(dc, &pixel), &pixel);
   context = wglCreateContext(dc);
   wglMakeCurrent(dc, context);

   dxgi = wgl_dxgi_new(window);
   check(dxgi != NULL, "the window is presented through DXGI");
   if (dxgi)
   {
      check(wgl_dxgi_framebuffer(dxgi) != 0, "OpenGL draws into a framebuffer of the presenter's");
      shows_upright(window, dxgi, 64, 48, "64x48");
      /* The presenter follows the window at the frame after it changes. */
      SetWindowPos(window, NULL, x, 0, 80, 60, SWP_NOZORDER | SWP_NOACTIVATE);
      pump();
      draw(dxgi, 80, 60);
      wgl_dxgi_present(dxgi, 1);
      shows_upright(window, dxgi, 80, 60, "80x60");
      wgl_dxgi_free(dxgi);
   }
   wglMakeCurrent(NULL, NULL);
   wglDeleteContext(context);
   ReleaseDC(window, dc);
   DestroyWindow(window);
   if (failures)
      return 1;
   printf("a fullscreen game's picture is shown upright through DXGI, and follows its window\n");
   return 0;
}
