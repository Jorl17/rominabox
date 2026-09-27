"""Check whether a process is still running without sending it a signal.

On POSIX systems we make that check with `os.kill(pid, 0)`. On Windows,
signal 0 is CTRL_C_EVENT, so with the same call we would send a console
Ctrl+C, which can interrupt unrelated processes that share the console.
"""

from __future__ import annotations

import os


def alive(pid: int) -> bool:
    """Return whether a process with this id exists, whichever user started it."""
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes

        process_query_limited_information = 0x1000
        still_active = 259
        error_access_denied = 5
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.OpenProcess.restype = wintypes.HANDLE
        handle = kernel32.OpenProcess(process_query_limited_information, False, pid)
        if not handle:
            # Another user's process exists, but we may not query it.
            return ctypes.get_last_error() == error_access_denied
        try:
            code = wintypes.DWORD()
            if not kernel32.GetExitCodeProcess(handle, ctypes.byref(code)):
                return True
            return code.value == still_active
        finally:
            kernel32.CloseHandle(handle)
    elif os.name == "posix":
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        return True
    raise NotImplementedError(f"no process check for os.name {os.name!r}")
