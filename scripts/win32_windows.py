"""Read a game's windows on Windows through Win32, for the checks that inspect
a game's window and query it without showing it to anyone.
"""

from __future__ import annotations

import ctypes
import time
from ctypes import wintypes
from pathlib import Path


# The application and relaunch properties of the shell (propkey.h). We set
# them so that a taskbar button pinned from a window reopens the game.
APP_USER_MODEL = "{9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}"
TASKBAR_LABELS = {"id": 5, "relaunch command": 2, "relaunch icon": 3, "relaunch name": 4}


def windows_of(folder: Path) -> list[int]:
    """Return the main RetroArch windows of programs inside `folder`."""
    user32 = ctypes.WinDLL("user32")
    kernel32 = ctypes.WinDLL("kernel32")
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
                                                    ctypes.POINTER(wintypes.DWORD)]
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    found: list[int] = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def each(window, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(window, ctypes.byref(owner))
        name = ctypes.create_unicode_buffer(64)
        user32.GetClassNameW(window, name, len(name))
        process = kernel32.OpenProcess(0x1000, False, owner.value)  # QUERY_LIMITED_INFORMATION
        if process and name.value == "RetroArch":
            image = ctypes.create_unicode_buffer(32768)
            size = wintypes.DWORD(len(image))
            if kernel32.QueryFullProcessImageNameW(process, 0, image, ctypes.byref(size)):
                if Path(image.value).resolve().is_relative_to(folder.resolve()):
                    found.append(window)
        if process:
            kernel32.CloseHandle(process)
        return True

    user32.EnumWindows(each, 0)
    return found


def taskbar_labels(window: int) -> dict[str, str | None]:
    """Return the window's properties for the taskbar as text, or None if unset."""
    class Guid(ctypes.Structure):
        _fields_ = [("data", ctypes.c_ubyte * 16)]

    class PropertyKey(ctypes.Structure):
        _fields_ = [("fmtid", Guid), ("pid", wintypes.DWORD)]

    class PropVariant(ctypes.Structure):
        _fields_ = [("vt", ctypes.c_ushort), ("reserved", ctypes.c_ushort * 3),
                    ("value", ctypes.c_void_p), ("more", ctypes.c_void_p)]

    ole32 = ctypes.WinDLL("ole32")
    shell32 = ctypes.WinDLL("shell32")
    ole32.CoInitialize(None)
    store_iid = Guid()
    ole32.IIDFromString("{886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99}", ctypes.byref(store_iid))
    store = ctypes.c_void_p()
    if shell32.SHGetPropertyStoreForWindow(wintypes.HWND(window), ctypes.byref(store_iid),
                                           ctypes.byref(store)) != 0:
        raise SystemExit("the game's window has no property store")
    methods = ctypes.cast(ctypes.cast(store, ctypes.POINTER(ctypes.c_void_p))[0],
                          ctypes.POINTER(ctypes.c_void_p))
    get_value = ctypes.WINFUNCTYPE(ctypes.c_long, ctypes.c_void_p, ctypes.POINTER(PropertyKey),
                                   ctypes.POINTER(PropVariant))(methods[5])
    release = ctypes.WINFUNCTYPE(ctypes.c_ulong, ctypes.c_void_p)(methods[2])
    labels: dict[str, str | None] = {}
    for label, pid in TASKBAR_LABELS.items():
        key = PropertyKey(pid=pid)
        ole32.IIDFromString(APP_USER_MODEL, ctypes.byref(key.fmtid))
        value = PropVariant()
        text = None
        if get_value(store, ctypes.byref(key), ctypes.byref(value)) == 0 and value.vt == 31:  # VT_LPWSTR
            text = ctypes.wstring_at(value.value)
        ole32.PropVariantClear(ctypes.byref(value))
        labels[label] = text
    release(store)
    return labels


# Window styles the checks read (winuser.h).
WS_EX_LAYERED = 0x00080000
WS_EX_TRANSPARENT = 0x00000020
WS_EX_NOACTIVATE = 0x08000000
WS_EX_TOOLWINDOW = 0x00000080


def _user32():
    user32 = ctypes.WinDLL("user32")
    user32.GetWindowLongPtrW.argtypes = [wintypes.HWND, ctypes.c_int]
    user32.GetWindowLongPtrW.restype = ctypes.c_ssize_t
    user32.GetLayeredWindowAttributes.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD),
                                                  ctypes.POINTER(ctypes.c_ubyte), ctypes.POINTER(wintypes.DWORD)]
    user32.GetForegroundWindow.restype = wintypes.HWND
    user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    return user32


def extended_style(window: int) -> int:
    return _user32().GetWindowLongPtrW(window, -20)  # GWL_EXSTYLE


def layered_alpha(window: int) -> int | None:
    """Return the opacity of the whole window, 0 to 255, or None when unset."""
    key = wintypes.DWORD()
    alpha = ctypes.c_ubyte()
    flags = wintypes.DWORD()
    if not _user32().GetLayeredWindowAttributes(window, ctypes.byref(key), ctypes.byref(alpha), ctypes.byref(flags)):
        return None
    return alpha.value if flags.value & 0x2 else None  # LWA_ALPHA


def is_foreground(window: int) -> bool:
    return (_user32().GetForegroundWindow() or 0) == window


def window_of_running_game(app: Path, process, log: Path | None, seconds: float = 60) -> int | None:
    """Return the main window of the game `process` started from `app`, once the
    core is loaded according to its log, or None if it never appears or the game ends."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline and process.poll() is None:
        text = log.read_text(encoding="utf-8", errors="replace") if log and log.exists() else ""
        windows = windows_of(app) if "Loading dynamic libretro core" in text else []
        if windows:
            return windows[0]
        time.sleep(0.4)
    return None


def close(window: int) -> None:
    """Return the message from the window's close button."""
    _user32().PostMessageW(window, 0x0010, 0, 0)  # WM_CLOSE
