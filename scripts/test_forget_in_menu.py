"""Check that choosing UNINSTALL on Windows or RESET on macOS in a running
game's menu removes the game's data on this computer and keeps the game.

We export the generated cartridge to open at its menu, with the player of
this checkout (built with the menu's script driver) and the launcher built
from this tree, and we open it as a person would, on Windows through its
single program. We play it once, so that it has data. Then we use a menu
script to choose Options, the screen in the design for forgetting the game
on this platform, and then the confirm button on it, and the game closes by
itself. What it kept is there before and gone after:

- Windows: its sandbox, registered and with the game's data in its folder,
  and its unpacked copy. The program stays.
- macOS: its data folder. The app stays.

    python scripts/test_forget_in_menu.py

This does not check that a person's pointer or keys reach the button
(the navigation tests), the words on the screen, or the builder's uninstaller.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import exported_game  # noqa: E402
import menu_shots  # noqa: E402

DESIGN = ROOT / "integrations/designs/native/design.json"
CONTRACT = ROOT / "vendor/retroarch/menu/drivers/rmlui/document_contract.inc"
TIMEOUT_SECONDS = 120


def button(role: str) -> str:
    """Return the element that opens the screen of `role` in a game for this
    platform, as declared in the design that the game is exported with."""
    for screen in json.loads(DESIGN.read_text(encoding="utf-8"))["screens"]:
        platforms = screen.get("platforms")
        if screen.get("role") == role and (not platforms or exported_game.PLATFORM in platforms):
            return screen["button"]
    raise SystemExit(f"{DESIGN} declares no {role} screen for {exported_game.PLATFORM}")


def element(name: str) -> str:
    """Return an element of the menu's document contract, by its name there."""
    found = re.search(rf'RIB_ELEMENT\({name}, "([^"]+)"', CONTRACT.read_text(encoding="utf-8"))
    if not found:
        raise SystemExit(f"{CONTRACT} declares no {name}")
    return found.group(1)


def windows_registered(sandbox: str) -> bool:
    """Return whether the sandbox named `sandbox` is registered for this user.
    The name of each registered sandbox is under the user's SID in Windows."""
    import ctypes
    import winreg
    from ctypes import wintypes

    userenv = ctypes.WinDLL("userenv")
    advapi = ctypes.WinDLL("advapi32")
    kernel = ctypes.WinDLL("kernel32")
    userenv.DeriveAppContainerSidFromAppContainerName.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(ctypes.c_void_p)]
    userenv.DeriveAppContainerSidFromAppContainerName.restype = ctypes.c_long
    advapi.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
    advapi.FreeSid.argtypes = [ctypes.c_void_p]
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    sid = ctypes.c_void_p()
    text = ctypes.c_void_p()
    if userenv.DeriveAppContainerSidFromAppContainerName(sandbox, ctypes.byref(sid)) != 0:
        raise SystemExit(f"Windows cannot name the sandbox {sandbox}")
    if not advapi.ConvertSidToStringSidW(sid, ctypes.byref(text)):
        raise SystemExit(f"Windows cannot spell the sandbox {sandbox}")
    spelled = ctypes.wstring_at(text.value)
    kernel.LocalFree(text)
    advapi.FreeSid(sid)
    key = rf"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppContainer\Mappings\{spelled}"
    try:
        winreg.OpenKey(winreg.HKEY_CURRENT_USER, key).Close()
    except FileNotFoundError:
        return False
    return True


def windows_kept(app: Path) -> dict:
    """Return the data of a Windows game on this computer. We read the names
    from the plan in its unpacked copy, so we call this while that copy exists."""
    sandbox = exported_game.storage_home(app)
    data = exported_game.data_dir_of(app)
    return {
        "the sandbox's registration": lambda: windows_registered(sandbox.name),
        "the sandbox's folder": sandbox.is_dir,
        "the game's data": data.is_dir,
        "the unpacked copy": app.is_dir,
    }


def macos_kept(app: Path) -> dict:
    """Return the data of a Mac game on this computer."""
    return {"the game's data": exported_game.data_dir_of(app).is_dir}


# By platform, the data of a game on this computer, each item with what it
# is and how we tell that it exists, and the game a person opens, which stays.
KEPT = {"windows": windows_kept, "macos": macos_kept}
OPENED = {
    "windows": menu_shots.program_of,
    "macos": exported_game.launcher_of,
}


def play(app: Path, opened: Path, script: list[str], shot: Path | None = None) -> str:
    """Run the game opened by `opened` once, quietly, with `script`, and return
    what it wrote to its error output."""
    environment = {**os.environ, exported_game.quiet_env(): "1", exported_game.SCRIPT_ENV: ",".join(script)}
    environment.pop("ROMINABOX_MENU_SHOT", None)
    if shot is not None:
        environment["ROMINABOX_MENU_SHOT"] = str(shot)
    with tempfile.TemporaryFile(mode="w+t") as errors:
        player = subprocess.Popen([str(opened)], env=environment, stdin=subprocess.DEVNULL,
                                  stdout=subprocess.DEVNULL, stderr=errors)
        try:
            player.wait(timeout=TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            raise menu_shots.PlayerTimeout(
                f"the game did not close within {TIMEOUT_SECONDS}s (pid {player.pid}); left running for inspection",
                app,
            ) from None
        errors.seek(0)
        return f"exit {player.returncode}\n{errors.read()}"


def main() -> int:
    import fetch_test_content

    if exported_game.PLATFORM not in KEPT:
        raise SystemExit(f"what a game keeps is not declared for {exported_game.PLATFORM}")
    cartridge, reason = fetch_test_content.locate("test-game")
    if cartridge is None:
        print(f"skipped: test-game not available ({reason})")
        return 0
    settings = {"title": "Forget In Menu", "startAtMenu": True}
    with menu_shots.build_a_game(cartridge, ROOT / "work/forget-in-menu", "gbc", settings) as app:
        kept = KEPT[exported_game.PLATFORM](app)
        opened = OPENED[exported_game.PLATFORM](app)
        # We play it once, as a person plays it before choosing to forget it.
        picture = ROOT / "work/test-output/forget-in-menu.png"
        shot = exported_game.shot_inside(app, picture)
        first = play(app, opened, ["wait:1"], shot)
        exported_game.carry_shot(shot, picture)
        missing = [name for name, there in kept.items() if not there()]
        if missing:
            print(first)
            print(f"FAIL once played, the game has no {', '.join(missing)}")
            return 1
        script = [button("options"), button("forget"), element("ForgetConfirm")]
        forgot = play(app, opened, script)
        left = [name for name, there in kept.items() if there()]
        failures = [f"{name} is still there" for name in left]
        if not opened.is_file():
            failures.append(f"{opened} is gone")
        for failure in failures:
            print(f"FAIL after {' -> '.join(script)}: {failure}")
        if failures:
            print(forgot)
            return 1
        print(f"{exported_game.PLATFORM}: {' -> '.join(script)} removed {', '.join(kept)}; "
              f"{opened.name} stays")
    return 0


if __name__ == "__main__":
    sys.exit(main())
