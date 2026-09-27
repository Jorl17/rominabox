"""Screenshot the menu from the game, not from a renderer standing in for it.

With `scripts/menu_states.py` we draw `menu.rml` through RmlUi, which is
enough for questions about layout and style. There we cannot see whether a
state is reachable through the bridge, because the bridge is not loaded. In
that script we set the classes ourselves and take a picture of the result,
so the picture shows a state that we set and not the menu at work.

Here we run the exported game unchanged, through its launcher, with two
environment variables:

  ROMINABOX_MENU_SCRIPT  comma-separated element ids and key: actions. We
                         click element ids through the bridge's listeners
                         and send key: actions as logical controller input
  ROMINABOX_MENU_SHOT    where to write the screenshot after the script

When the menu has settled, we write the picture and quit the game, so
nothing stays open. When an id is not in the document, we stop the run
instead of taking a picture of the wrong screen.

    python3 scripts/menu_shots.py --app "/path/to/Game.app"
    python3 scripts/menu_shots.py --rom game.md --design disc --palette carbon
    python3 scripts/menu_shots.py --rom game.md --palette amber
    python3 scripts/menu_shots.py --rom game.md --every-palette
    python3 scripts/menu_shots.py --rom game.md --only pause-menu,options
    python3 scripts/menu_shots.py --app "/path/to/Game.app" --check

This does not test window placement, focus or fullscreen behaviour, which
a person has to check. Here we test only what is on screen in the game.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from contextlib import ExitStack, contextmanager
from pathlib import Path
from typing import Iterator

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
from core_source import host_target  # noqa: E402
from directory_links import redirected  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "scripts/fixtures/menu-shots.json"
DIGESTS = ROOT / "scripts/fixtures/menu-shot-digests.json"

# A generous limit. The game quits as soon as the picture is written, and we
# use this limit only so that a stuck run cannot stop the tests forever.
TIMEOUT_SECONDS = 120


class PlayerTimeout(SystemExit):
    """The exported app may still be in use by a player we launched."""

    def __init__(self, message: str, app: Path) -> None:
        super().__init__(message)
        self.app = app


DATA_DIR = re.compile(r'^data_dir\t(.+)$', re.MULTILINE)
# The switch for a quiet automated launch, and the opt-out that a person
# testing by hand can set. We take the names from here in harnesses, and in
# the quiet tests we check them against the launcher (test_quiet.plan_check).
QUIET_ENV = "ROMINABOX_QUIET"
SOUND_ENV = "ROMINABOX_SOUND"


def quiet_env() -> str:
    return QUIET_ENV


def declared_palettes() -> list[str]:
    """Palette ids from desktop/designs.json, in the order they are declared.

    We read them instead of listing them here, so we take pictures of a new
    palette without a change to this code.
    """
    declared = json.loads((ROOT / "desktop/designs.json").read_text(encoding="utf-8"))["palettes"]
    names = [entry["id"] for entry in declared]
    if not names:
        raise SystemExit("desktop/designs.json declares no palettes")
    return names


def require_palette(name: str) -> str:
    declared = declared_palettes()
    if name not in declared:
        raise SystemExit(f"unknown palette '{name}'; declared: {', '.join(declared)}")
    return name


def declared_shots() -> dict[str, dict]:
    """Each named shot, with its menu actions and the game to take it in.

    A shot is a list of steps, or an object with `script` and the export
    settings required for that shot. We can reach most states in the menu of
    a game that opens at the menu. In that export there is nothing drawn over
    a running game.
    """
    declared = json.loads(SHOTS.read_text(encoding="utf-8"))["shots"]
    return {
        name: ({"script": entry} if isinstance(entry, list) else entry)
        for name, entry in declared.items()
    }


def _macos_launcher(app: Path) -> Path:
    """The bundle's main executable, which is the sandboxed player."""
    identifier = subprocess.run(
        [
            "/usr/bin/plutil",
            "-extract",
            "CFBundleExecutable",
            "raw",
            str(app / "Contents/Info.plist"),
        ],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    executable = app / "Contents/MacOS" / identifier
    if executable.is_file() and os.access(executable, os.X_OK):
        return executable
    raise SystemExit(f"no launcher inside {app}")


def _windows_launcher(app: Path) -> Path:
    """The program of the game, named after the game, in its folder."""
    executable = app / f"{app.name}.exe"
    if executable.is_file():
        return executable
    raise SystemExit(f"no launcher inside {app}")


def _macos_running(app: Path) -> str:
    """Processes whose command line contains the app, one "pid command" per line."""
    found = subprocess.run(["pgrep", "-fl", str(app)], capture_output=True, text=True, timeout=15)
    return "\n".join(line for line in found.stdout.splitlines() if "pgrep" not in line)


def _windows_running(app: Path) -> str:
    """Processes whose program lies inside the game's folder, one "pid program"
    per line."""
    import ctypes
    from ctypes import wintypes

    psapi = ctypes.WinDLL("psapi")
    kernel32 = ctypes.WinDLL("kernel32")
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
                                                    ctypes.POINTER(wintypes.DWORD)]
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    ids = (wintypes.DWORD * 4096)()
    written = wintypes.DWORD()
    if not psapi.EnumProcesses(ids, ctypes.sizeof(ids), ctypes.byref(written)):
        raise SystemExit("could not list the running processes")
    folder = app.resolve()
    found = []
    for pid in ids[: written.value // ctypes.sizeof(wintypes.DWORD)]:
        process = kernel32.OpenProcess(0x1000, False, pid)  # QUERY_LIMITED_INFORMATION
        if not process:
            continue
        image = ctypes.create_unicode_buffer(32768)
        size = wintypes.DWORD(len(image))
        if kernel32.QueryFullProcessImageNameW(process, 0, image, ctypes.byref(size)):
            if Path(image.value).resolve().is_relative_to(folder):
                found.append(f"{pid} {image.value}")
        kernel32.CloseHandle(process)
    return "\n".join(found)


def _macos_sandboxed(app: Path) -> bool:
    signed = subprocess.run(
        ["/usr/bin/codesign", "-d", "--entitlements", "-", str(app)],
        capture_output=True,
        text=True,
    )
    return "com.apple.security.app-sandbox" in signed.stdout + signed.stderr


IDENTITY = re.compile(r'^identity\t(.+)$', re.MULTILINE)
SANDBOX = re.compile(r'^sandbox\t1$', re.MULTILINE)


def _windows_sandboxed(app: Path) -> bool:
    """In a Windows game's launcher we set up the game's sandbox when the plan
    has that option set, as it has in every export."""
    return bool(SANDBOX.search(plan_text(app)))


def _macos_prepare_storage(app: Path) -> None:
    """Nothing. In the macOS tests we photograph a game's first launch with its
    shot folder made beforehand, and the picture arrives."""


def _windows_prepare_storage(app: Path) -> None:
    """Register the game's sandbox with a plan-only launch before we write
    anything into its storage. Registering a sandbox over an existing folder
    empties that folder, including any shot folder we made there, so no
    picture would arrive from a game's first shot."""
    if not _windows_sandboxed(app):
        return
    subprocess.run(
        [str(launcher_of(app))],
        env=dict(os.environ, ROMINABOX_PLAN_ONLY="1", **{quiet_env(): "1"}),
        stdin=subprocess.DEVNULL,
        capture_output=True,
        check=True,
        timeout=60,
    )


def _windows_sandbox_folder(app: Path) -> Path:
    """The sandbox folder of a Windows game, named as we name the sandbox in
    the launcher: the application id prefix declared for the launcher and
    the player in vendor/retroarch/rominabox_launch.h, and the game's identity.
    Inside, the game's per-user folder is its AC folder."""
    header = (ROOT / "vendor/retroarch/rominabox_launch.h").read_text(encoding="utf-8")
    prefix = re.search(r'#define RIB_GAME_APP_ID_PREFIX "([^"]+)"', header)
    identity = IDENTITY.search(plan_text(app))
    if not prefix or not identity:
        raise SystemExit(f"cannot name the sandbox of {app}")
    return Path(os.environ["LOCALAPPDATA"]) / "Packages" / f"{prefix.group(1)}{identity.group(1)}"


def home_for(app: Path) -> str:
    """The HOME of the exported game.

    In App Sandbox, HOME is inside the container. The plan's $user_data is
    Application Support in that HOME, as we resolve it in the launcher.
    """
    if not sandboxed(app):
        return str(Path.home())
    identifier = subprocess.run(
        [
            "/usr/bin/plutil",
            "-extract",
            "CFBundleIdentifier",
            "raw",
            str(app / "Contents/Info.plist"),
        ],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    return str(Path.home() / "Library/Containers" / identifier / "Data")


# What differs with the platform of the game: its launcher, the folder for
# its files, the per-user folder that $user_data stands for in the plan,
# whether it is sandboxed, the folder that must contain its per-game storage
# (in a sandbox, a macOS game's container or a Windows game's Packages
# folder, and otherwise nothing on macOS and the games folder in the per-user
# application data on Windows), the place of the player in the kit (the macOS
# kit from freeze-runtime-kit.mjs, the Windows kit from the recipe), and which
# processes of the game are still running.
APPS = {
    "macos": {
        "launcher": _macos_launcher,
        "resources": lambda app: app / "Contents/Resources",
        "user_data": lambda app: Path(home_for(app)) / "Library/Application Support",
        "sandboxed": _macos_sandboxed,
        "prepare_storage": _macos_prepare_storage,
        "storage_home": lambda app: Path(home_for(app)) if _macos_sandboxed(app) else None,
        "kit_player": lambda: "bin/retroarch",
        "running": _macos_running,
    },
    "windows": {
        "launcher": _windows_launcher,
        "resources": lambda app: app / "Resources",
        "user_data": lambda app: (_windows_sandbox_folder(app) / "AC" if _windows_sandboxed(app)
                                  else Path(os.environ["LOCALAPPDATA"])),
        "sandboxed": _windows_sandboxed,
        "prepare_storage": _windows_prepare_storage,
        "storage_home": lambda app: (_windows_sandbox_folder(app) if _windows_sandboxed(app)
                                     else Path(os.environ["LOCALAPPDATA"]) / "ROM-in-a-Box" / "Games"),
        "kit_player": lambda: native_build.recipe()["kit"][host_target()]["files"]["player"]["at"],
        "running": _windows_running,
    },
}
PLATFORM = host_target().split("-", 1)[0]


def _app() -> dict:
    if PLATFORM not in APPS:
        raise SystemExit(f"no exported game is declared for {PLATFORM}")
    return APPS[PLATFORM]


def launcher_of(app: Path) -> Path:
    return _app()["launcher"](app)


def sandboxed(app: Path) -> bool:
    return _app()["sandboxed"](app)


# The games whose storage we have prepared in this run, once each.
_prepared: set[Path] = set()


def prepare_storage(app: Path) -> None:
    """The game's storage as it is after the first launch, before we write into
    it from a harness."""
    if app.resolve() in _prepared:
        return
    _app()["prepare_storage"](app)
    _prepared.add(app.resolve())


def prepared_storage(app: Path) -> Path | None:
    """The game's own storage, prepared before we write into it from a
    harness, or None for a game without one."""
    data = data_dir_of(app)
    if data is not None:
        prepare_storage(app)
    return data


def shot_inside(app: Path, target: Path) -> Path:
    """The path where we tell the game to write the picture for `target`.

    A sandboxed game has write access only inside its own storage, not in
    this repository. So the picture goes into the game's storage, and we
    move it out with `carry_shot`. With a path in the tree, every shot would
    end in "failed to open file for writing", because of the sandbox.
    """
    data = prepared_storage(app)
    if data is None or not sandboxed(app):
        return target
    inside = data / "shots" / target.name
    inside.parent.mkdir(parents=True, exist_ok=True)
    inside.unlink(missing_ok=True)
    return inside


def carry_shot(inside: Path, target: Path) -> None:
    """Move the picture from the game's own storage to `target`."""
    if inside != target and inside.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(inside), str(target))


def running_from(app: Path) -> str:
    """The game's processes still running, one per line, or empty when none is."""
    return _app()["running"](app)


def storage_home(app: Path) -> Path | None:
    """The folder a game's own storage must lie inside, or None when the
    game's storage is not contained."""
    return _app()["storage_home"](app)


def resources_of(app: Path) -> Path:
    """The folder of an exported game's own files."""
    return _app()["resources"](app)


def plan_text(app: Path) -> str:
    path = resources_of(app) / "launch.plan"
    if path.is_file():
        return path.read_text(encoding="utf-8")
    return ""


def data_dir_of(app: Path) -> Path | None:
    """The per-game storage, as we compute it in the launcher."""
    found = DATA_DIR.search(plan_text(app))
    if not found:
        return None
    return Path(found.group(1).replace("$user_data", str(_app()["user_data"](app))))


def log_of(app: Path) -> Path | None:
    """The file to which we send the player's output in the launcher."""
    data = data_dir_of(app)
    if data is None:
        return None
    return data / "logs/launch.log"


def take(app: Path, name: str, script: list[str], output: Path,
         config: dict | None = None, *, reset_settings: bool = True) -> str:
    """Run the game to a state and screenshot it. Empty string on success."""
    # An absolute path, because we tell the game where to write and its working
    # directory is not this one. With a relative output directory, we get the
    # error "failed to open file for writing" for every shot.
    target = output.resolve() / f"{name}.png"
    target.unlink(missing_ok=True)
    # What we delete for a shot is in the game's storage, and through a link
    # it would be the files of someone else.
    data = prepared_storage(app)
    if data is not None:
        for folder in (data, data / "logs", data / "remaps"):
            if redirected(folder):
                return f"refusing a link in the game's storage: {folder}"
    log = log_of(app)
    if log and log.exists():
        log.unlink()

    # We start each shot from the game as it is shipped. Choosing a pad in the
    # picker writes it to the per-game override, so without this step the shot
    # of the six-button pad would change the pictures of every later shot.
    if name.startswith("achievements-"):
        if data is None:
            return "account shots require managed per-game storage"
        session = data / "achievements.session"
        if session.exists() or redirected(session):
            return "account shots require signed-out game storage; achievements.session is present"
    if data and reset_settings:
        (data / "controls.cfg").unlink(missing_ok=True)
        for remap in (data / "remaps").rglob("*.rmp"):
            remap.unlink()
        # We save the positions of the menu switches in an export across runs.
        for switch in data.glob("toggle-*"):
            switch.unlink()

    # In a shot we can set other run settings, but not `pause_nonactive`. We
    # set ROMINABOX_MENU_SHOT in every run, and with it the console keeps
    # running in the launcher. Writing the key into controls.cfg would
    # override the setting the author chose the next time someone opens the game.
    config = {
        key: value
        for key, value in (config or {}).items()
        if key != "pause_nonactive"
    }
    if data and config:
        data.mkdir(parents=True, exist_ok=True)
        (data / "controls.cfg").write_text(
            "".join(f'{key} = "{value}"\n' for key, value in config.items()),
            encoding="utf-8",
            newline="\n",
        )

    inside = shot_inside(app, target)

    with (
        tempfile.TemporaryFile(mode="w+t") as stdout_capture,
        tempfile.TemporaryFile(mode="w+t") as stderr_capture,
    ):
        player = subprocess.Popen(
            [str(launcher_of(app))],
            stdout=stdout_capture,
            stderr=stderr_capture,
            text=True,
            env=dict(
                os.environ,
                ROMINABOX_MENU_SCRIPT=",".join(script),
                ROMINABOX_MENU_SHOT=str(inside),
                # No sound, and a transparent window. Without this, we would open
                # CoreAudio during a shot and leave a window on the display.
                **{quiet_env(): "1"},
            ),
        )
        try:
            player.wait(timeout=TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            log_tail = log.read_text(encoding="utf-8", errors="replace")[-800:] if log and log.exists() else ""
            stderr_capture.seek(0, os.SEEK_END)
            stderr_capture.seek(max(stderr_capture.tell() - 800, 0))
            partial = stderr_capture.read()
            raise PlayerTimeout(
                f"player timed out after {TIMEOUT_SECONDS}s (pid {player.pid}); "
                "left running for inspection\n"
                f"{log_tail or partial}",
                app,
            ) from None
        stderr_capture.seek(0)
        stderr = stderr_capture.read()
    carry_shot(inside, target)
    written = log.read_text(encoding="utf-8", errors="replace") if log and log.exists() else stderr
    # Copy the evidence next to its picture before we overwrite it next launch.
    (output / f"{name}.log").write_text(written, encoding="utf-8", newline="\n")

    # We read this from the player report. Otherwise, when no click in the
    # script happened, we would take a picture of whatever was on screen.
    for line in written.splitlines():
        if "names no element" in line:
            return line.split("[RIB]")[-1].strip()
    if not target.exists():
        tail = "\n".join(written.strip().splitlines()[-6:])
        return f"no screenshot was written (exit {player.returncode})\n{tail}"
    return ""


KIT = ROOT / "desktop/src-tauri/resources/runtime"
# We build it here and check that it comes from this checkout, because every
# worktree shares one cargo target, so the binary next to the manifest may be
# out of date or from another checkout. See scripts/built.py.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from built import cli as _cli  # noqa: E402
import player_build  # noqa: E402
from core_source import core_source  # noqa: E402


def command() -> Path:
    """The exporter, which we look up only when we build a game.

    We do not build it on import, because the signature check imports this
    module without exporting, and during a build we take the cargo lock
    shared by every other checkout.
    """
    return _cli()


# One namespace for every shot, so that we do not create a separate
# container for each palette. We must never delete containers under other
# names from a script.
SHOT_BUNDLE_PREFIX = "app.rominabox.game.shots"


def shot_bundle_prefix(_workspace: Path) -> str:
    """One namespace for every shot, not one per build directory.

    Build directories such as menu-shots-build-0 and shaderstate-build give
    different bundle ids, so we would leave one more container per palette,
    and we must not delete those from a script. In a worktree we use its
    ROMINABOX_GAME_BUNDLE_PREFIX, and elsewhere SHOT_BUNDLE_PREFIX, never
    the identity of a player's game.
    """
    published = os.environ.get("ROMINABOX_GAME_BUNDLE_PREFIX", "").strip()
    if published:
        return published
    return SHOT_BUNDLE_PREFIX


def capture_export_entitlements(app: Path, destination: Path) -> None:
    """Save the sandbox we signed on export before replacing the player binary.

    Replacing Contents/MacOS/retroarch discards that signature, and after
    that there is nothing left to read and put back.
    """
    dumped = subprocess.run(
        ["/usr/bin/codesign", "-d", "--entitlements", str(destination), "--xml", str(app)],
        capture_output=True,
    )
    text = destination.read_text(encoding="utf-8") if destination.is_file() else ""
    if dumped.returncode != 0 or "com.apple.security.app-sandbox" not in text:
        detail = dumped.stderr.decode(errors="replace")[-400:]
        raise SystemExit(
            "the export is not sandboxed, so a shot would not be either\n" + detail
        )


def resign_replaced_player(app: Path, entitlements: Path) -> None:
    """Sign the replacement with the entitlements we wrote on export.

    Signing the new binary with a bare `codesign --sign -` drops the sandbox,
    and `--preserve-metadata=entitlements` on the bundle does not restore it.
    The game's storage would then be under
    ~/Library/Application Support/ROM-in-a-Box instead of in a container.
    """
    if not entitlements.is_file():
        raise SystemExit(f"no entitlements to re-sign with at {entitlements}")
    library = app / "Contents/MacOS/librominabox-launch.dylib"
    retroarch = launcher_of(app)
    if library.is_file():
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(library)], check=True)
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(retroarch)], check=True)
    subprocess.run(
        [
            "/usr/bin/codesign", "--force", "--sign", "-",
            "--entitlements", str(entitlements),
            str(app),
        ],
        check=True,
    )


def _runs_scripts(build: Path) -> bool:
    info = build / "build-info.json"
    return info.is_file() and json.loads(info.read_text(encoding="utf-8")).get("capabilities", {}).get(
        "menuScript"
    ) is True


def built_player() -> Path:
    """The player for a launched test: one built in this checkout with the
    script driver of the menu.

    A shot must show the fork as it is now, not as it was when we froze the
    kit, and after a change to the player we build it in this checkout. Each
    checkout has a separate build directory, so in a worktree where we built
    the player, the shots show that player. We drive the menu with a script
    in every launched test, and only a test build has the driver, so we never
    fall back to the player in the kit, which is the one we ship in games.
    """
    how = (
        "build one with ROMINABOX_MENU_SCRIPT_BUILD=1 python3 "
        "scripts/build_player.py <absolute dir>"
    )
    if os.environ.get("ROMINABOX_TEST_BUILD"):
        build = player_build.selected_build()
        if not _runs_scripts(build):
            raise SystemExit(f"{build} has no menu script driver; {how}")
        return player_build.player_in(build)
    builds = sorted(
        (
            player_build.player_in(build)
            for build in (ROOT / "work").glob("fork-build-*")
            if _runs_scripts(build) and player_build.player_in(build).is_file()
        ),
        key=lambda entry: entry.stat().st_mtime,
    )
    if not builds:
        raise SystemExit(f"no player that runs menu scripts in work/fork-build-*; {how}")
    return builds[-1]


def _build_a_game(
    rom: Path,
    workspace: Path,
    run_dir: Path,
    system: str = "megadrive",
    settings: dict | None = None,
    design: str = "native",
    palette: str = "blue",
) -> Path:
    """Export a game from the tree as it is now, and return the app.

    To take a picture of a change by hand, someone has to assemble a kit,
    remember which pieces are out of date, export, and replace the player
    binary, and each of these four steps can go wrong. The kit is a build
    output, so we refresh it from the tree here instead of trusting it.

    All shots use one bundle namespace, so we do not make another container
    for a second palette. Two checkouts stay apart only when
    ROMINABOX_GAME_BUNDLE_PREFIX is set in each of them.
    """
    settings = dict(settings or {})
    # In a shot, or in the palette loop, we can set the theme and the palette
    # like any other export setting. We copy the package for that export.
    design = str(settings.get("theme", design))
    palette = str(settings.get("palette", palette))
    kit = run_dir / "kit"
    shutil.copytree(KIT, kit, symlinks=True)

    # Copy the design as it is in this tree, not as when we froze the kit.
    # In the exporter we resolve Native and the selected design from this tree.
    # We also copy Native into menu-assets, because in one shot path we read
    # the controller art from there.
    for package_name in dict.fromkeys(("native", design)):
        package = ROOT / "integrations/designs" / package_name
        if not package.is_dir():
            raise SystemExit(f"no design package at {package}")
        staged_design = kit / "designs" / package_name
        staged_design.mkdir(parents=True, exist_ok=True)
        for document in package.iterdir():
            if document.is_file():
                shutil.copyfile(document, staged_design / document.name)
                if package_name == "native":
                    shutil.copyfile(document, kit / "menu-assets" / document.name)
    player = kit / _app()["kit_player"]()
    shutil.copyfile(built_player(), player)
    player.chmod(0o755)

    out = run_dir / "exported"
    out.mkdir(parents=True)
    request = {
        "rom": str(rom),
        "title": "Shot Subject",
        "system": system,
        "showMenu": True,
        "startAtMenu": True,
        "theme": design,
        "palette": palette,
        "menuSounds": "off",
        "splash": False,
        "advancedEmulatorAccess": False,
        # An account shot has the account option set next to its script. The other
        # visual baselines do not include the account menu entries.
        "includeAchievements": False,
        "outputDir": str(out),
        "target": PLATFORM,
        "runtimeKit": str(kit),
        # The kit contains no cores. In an export we take the core from the local
        # core cache, as we do from the cache of the builder.
        "coreCache": str(core_source()),
    }
    request.update(settings)
    result = subprocess.run(
        [str(command()), "export"],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        env=dict(os.environ, ROMINABOX_GAME_BUNDLE_PREFIX=shot_bundle_prefix(workspace)),
    )
    if result.returncode != 0:
        raise SystemExit(f"could not export a game to shoot:\n{result.stdout[-900:]}")
    written = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    app = next((Path(event["result"]["appPath"]) for event in written if event.get("type") == "result"), None)
    if app is None or not app.is_dir():
        raise SystemExit(f"the export wrote no app into {out}")
    return app


@contextmanager
def build_a_game(
    rom: Path,
    workspace: Path,
    system: str = "megadrive",
    settings: dict | None = None,
    design: str = "native",
    palette: str = "blue",
) -> Iterator[Path]:
    """Keep one generated export only until the end of the block.

    The app may still be in use by a player that timed out. In that case
    only, keep the temporary directory and print its location for inspection.
    """
    run = os.environ.get("ROMINABOX_SCRATCH_RUN", "direct")
    if not run or "/" in run or "\\" in run or ".." in run:
        raise ValueError(f"scratch run id must be one path component, got {run!r}")
    run_dir = Path(tempfile.mkdtemp(prefix=f"rominabox-menu-shots-{run}-"))
    created = run_dir.lstat()
    keep = False
    try:
        app = _build_a_game(rom, workspace, run_dir, system, settings, design, palette)
        try:
            yield app
        except PlayerTimeout as error:
            keep = error.app == app
            if keep:
                print(f"retained timed-out player's export for inspection: {run_dir}", file=sys.stderr)
            raise
    finally:
        if not keep:
            current = run_dir.lstat()
            if (
                not stat.S_ISDIR(current.st_mode)
                or current.st_dev != created.st_dev
                or current.st_ino != created.st_ino
            ):
                raise RuntimeError(f"temporary export changed ownership: {run_dir}")
            shutil.rmtree(run_dir)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, help="an exported .app to shoot")
    parser.add_argument(
        "--rom",
        type=Path,
        help="export a game from this ROM first, using the tree as it is now",
    )
    parser.add_argument("--system", default="megadrive", help="the console --rom is for")
    parser.add_argument("--design", default="native", help="which design to export")
    parser.add_argument("--palette", help="export this declared palette (default: blue)")
    parser.add_argument(
        "--every-palette",
        action="store_true",
        help="export and shoot every palette desktop/designs.json declares",
    )
    parser.add_argument(
        "--only",
        help="comma-separated shot names to take, instead of every declared shot",
    )
    parser.add_argument("output", type=Path, nargs="?", default=ROOT / "work/menu-shots")
    parser.add_argument("--record", action="store_true", help="record what each shot looks like")
    parser.add_argument("--check", action="store_true", help="fail if a shot changed")
    arguments = parser.parse_args()

    shots = declared_shots()
    if arguments.every_palette and arguments.palette:
        raise SystemExit("pass --palette or --every-palette, not both")
    if arguments.every_palette and not arguments.rom:
        raise SystemExit(
            "--every-palette exports each palette itself, so pass --rom. "
            "One --app is already one palette."
        )
    palettes = (
        declared_palettes()
        if arguments.every_palette
        else [require_palette(arguments.palette or "blue")]
    )
    package = ROOT / "integrations/designs" / arguments.design
    if not package.is_dir():
        raise SystemExit(f"no design package at {package}")
    if not arguments.app:
        if not arguments.rom:
            raise SystemExit("give --app an exported game, or --rom to export one first")
    elif not arguments.app.exists():
        raise SystemExit(f"no exported game at {arguments.app}")
    arguments.output.mkdir(parents=True, exist_ok=True)

    # We make one export for each set of export settings, not one per shot,
    # because the build is slow and most shots use the same game.
    exported: dict[str, Path] = {}

    def game_for(settings: dict) -> Path:
        if not arguments.rom:
            return arguments.app
        key = json.dumps(settings, sort_keys=True)
        if key not in exported:
            workspace = ROOT / "work" / f"menu-shots-build-{len(exported)}"
            exported[key] = export_stack.enter_context(
                build_a_game(arguments.rom, workspace, arguments.system, settings)
            )
            print(f"  built    {exported[key].name} {key if settings else ''}")
        return exported[key]

    if arguments.only:
        wanted = [name.strip() for name in arguments.only.split(",") if name.strip()]
        unknown = [name for name in wanted if name not in shots]
        if unknown:
            raise SystemExit(f"no such shot(s): {', '.join(unknown)}")
        shots = {name: shots[name] for name in wanted}

    failures: list[str] = []
    digests: dict[str, str] = {}
    nested = len(palettes) > 1
    with ExitStack() as export_stack:
        for palette in palettes:
            destination = arguments.output / palette if nested else arguments.output
            destination.mkdir(parents=True, exist_ok=True)
            for name, shot in shots.items():
                script = shot["script"]
                config = shot.get("config")
                # We key the export of the shot by the palette and its capability setting.
                settings = {
                    key: value
                    for key, value in shot.items()
                    # "inMotion" is about how we compare this shot, not how we
                    # export the game.
                    if key not in ("script", "config", "inMotion")
                }
                settings["palette"] = palette
                settings["theme"] = arguments.design
                key = f"{palette}/{name}" if nested else name
                problem = take(game_for(settings), name, script, destination, config)
                if problem:
                    print(f"  FAILED  {key}: {problem}", file=sys.stderr)
                    failures.append(key)
                    continue
                # A shot during an animation has no stable picture. We take this one
                # while the notice is fading out, so two runs differ slightly. We
                # still take it, so we notice when the state can no longer be
                # reached, but we do not compare its picture.
                if not shots[name].get("inMotion"):
                    digests[key] = hashlib.sha256(
                        (destination / f"{name}.png").read_bytes()
                    ).hexdigest()[:16]
                print(f"  {key:<28}{' -> '.join(script) or '(the menu as it opens)'}")

    # A shot we could not take and a shot that changed are two different
    # results. Report every failed state as well as every changed picture.
    if failures:
        print(f"\n{len(failures)} shot(s) failed: {', '.join(failures)}", file=sys.stderr)
        if not arguments.check:
            return 1

    if arguments.record:
        if failures:
            print(
                "nothing was recorded: a run that could not take every shot "
                "would drop the ones it missed.",
                file=sys.stderr,
            )
            return 1
        # We merge instead of replacing. A run with --only has no results for
        # the other shots, and writing only its results would drop theirs.
        kept = json.loads(DIGESTS.read_text(encoding="utf-8")) if DIGESTS.exists() else {}
        kept.update(digests)
        DIGESTS.write_text(json.dumps(kept, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
        print(f"\nrecorded {len(digests)} shots -> {DIGESTS.name}")
        return 0

    if arguments.check:
        if not DIGESTS.exists():
            raise SystemExit(f"no recorded shots at {DIGESTS}; run --record first")
        expected = json.loads(DIGESTS.read_text(encoding="utf-8"))
        changed = [n for n, d in digests.items() if expected.get(n) != d]
        for name in changed:
            print(f"  CHANGED {name}", file=sys.stderr)
        if changed:
            print(
                "\nThe game draws a menu state differently. Look at the pictures "
                "before re-recording.",
                file=sys.stderr,
            )
        if changed or failures:
            return 1
        print(f"\n{len(digests)} shots unchanged")
        return 0

    print(f"\n{len(digests)} shots across {len(palettes)} palette(s) -> {arguments.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
