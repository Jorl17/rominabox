/* The Windows half of test_quiet_window.m: in an automated run the window is
 * transparent, ignores clicks, is never activated and has no taskbar button,
 * and with the hands-on switch the window is unchanged. We create the window
 * and never show it. */
#include <windows.h>
#include <stdio.h>

#include "gfx/common/win32_quiet_window.h"
#include "test_environment.h"

#define QUIET_STYLES (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)

static HWND new_window(void)
{
   return CreateWindowExW(0, L"RibQuietProbe", L"probe", WS_OVERLAPPEDWINDOW,
         100, 100, 640, 480, NULL, NULL, GetModuleHandleW(NULL), NULL);
}

static int fail(const char *what)
{
   fprintf(stderr, "FAIL %s\n", what);
   return 1;
}

int main(void)
{
   WNDCLASSW type = {0};
   HWND window;
   LONG_PTR ordinary;
   BYTE alpha = 255;
   DWORD flags = 0;

   type.lpfnWndProc = DefWindowProcW;
   type.hInstance = GetModuleHandleW(NULL);
   type.lpszClassName = L"RibQuietProbe";
   if (!RegisterClassW(&type) || !(window = new_window()))
      return fail("could not make a window to check");
   ordinary = GetWindowLongPtrW(window, GWL_EXSTYLE);

   test_unsetenv("ROMINABOX_QUIET");
   test_unsetenv("ROMINABOX_SHOW_WINDOW");
   if (rib_session_window_hidden())
      return fail("a person's launch is taken for an automated one");
   rominabox_prepare_test_window(window);
   if (GetWindowLongPtrW(window, GWL_EXSTYLE) != ordinary)
      return fail("a person's window was changed");

   test_setenv("ROMINABOX_QUIET", "1");
   if (!rib_session_window_hidden())
      return fail("an automated launch is not hidden");
   rominabox_prepare_test_window(window);
   if ((GetWindowLongPtrW(window, GWL_EXSTYLE) & QUIET_STYLES) != QUIET_STYLES)
      return fail("the quiet window can be clicked, activated or shown in the taskbar");
   if (!GetLayeredWindowAttributes(window, NULL, &alpha, &flags) || !(flags & LWA_ALPHA) || alpha != 0)
      return fail("the quiet window can become visible");
   if (IsWindowVisible(window))
      return fail("the probe's window was shown");
   DestroyWindow(window);

   test_setenv("ROMINABOX_SHOW_WINDOW", "1");
   if (!(window = new_window()))
      return fail("could not make a second window to check");
   if (rib_session_window_hidden())
      return fail("the hands-on switch does not show an automated run");
   rominabox_prepare_test_window(window);
   if (GetWindowLongPtrW(window, GWL_EXSTYLE) != ordinary)
      return fail("the hands-on switch still changed the window");
   DestroyWindow(window);
   puts("quiet window stays transparent; ordinary style unchanged");
   return 0;
}
