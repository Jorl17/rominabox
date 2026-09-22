"""Screenshot the menu from the game, not from a renderer standing in for it.

With `scripts/menu_states.py` we draw `menu.rml` through RmlUi, which is
enough for questions about layout and style. There we cannot see whether a
state is reachable through the bridge, because the bridge is not loaded. In
that script we set the classes ourselves and take a picture of the result,
so the picture shows a state that we set and not the menu at work.

Here we run the exported game unchanged, through its launcher, with two
environment variables:

  ROMINABOX_MENU_SCRIPT  element ids, comma separated, which we click one per
                         frame through the listeners of the bridge, the same
                         path as a click from a player
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
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "scripts/fixtures/menu-shots.json"
DIGESTS = ROOT / "scripts/fixtures/menu-shot-digests.json"

# A generous limit. The game quits as soon as the picture is written, and we
# use this limit only so that a stuck run cannot stop the tests forever.
TIMEOUT_SECONDS = 120

DATA_DIR = re.compile(r'^data_dir\t(.+)$', re.MULTILINE)


def declared_palettes() -> list[str]:
    """Palette ids from desktop/designs.json, in the order they are declared.

    We read them instead of listing them here, so we take pictures of a new
    palette without a change to this code.
    """
    declared = json.loads((ROOT / "desktop/designs.json").read_text())["palettes"]
    names = [entry["id"] for entry in declared]
    if not names:
        raise SystemExit("desktop/designs.json declares no palettes")
    return names


def require_palette(name: str) -> str:
    declared = declared_palettes()
    if name not in declared:
        raise SystemExit(f"unknown palette '{name}'; declared: {', '.join(declared)}")
    return name


def achievement_request(directory: Path | None) -> dict:
    """The achievement settings we pass to the export.

    The list and its badges come from the service and are not in this
    repository. Without a directory we export a game with no achievements
    screen, and then we report the achievement shots as failed instead of
    taking pictures of the pause menu.
    """
    if directory is None:
        return {}
    catalog = directory / "achievements.json"
    if not catalog.is_file():
        raise SystemExit(
            f"{catalog} is not there. Fetch it first:\n"
            "  rominabox-cli achievements <<< '{\"gameId\": N, \"into\": \"<dir>\"}'"
        )
    return {
        "gameId": json.loads(catalog.read_text())["gameId"],
        "bundle": True,
        "catalog": str(catalog),
        "badges": str(directory / "badges"),
    }


def declared_shots() -> dict[str, dict]:
    """Each named shot, with what to click and the game to take it in.

    A shot is a list of steps, or an object with `script` and the export
    settings required for that shot. We can reach most states in the menu of
    a game that opens at the menu. In that export there is nothing drawn over
    a running game.
    """
    declared = json.loads(SHOTS.read_text())["shots"]
    return {
        name: ({"script": entry} if isinstance(entry, list) else entry)
        for name, entry in declared.items()
    }


def launcher_of(app: Path) -> Path:
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


def plan_text(app: Path) -> str:
    path = app / "Contents/Resources/launch.plan"
    if path.is_file():
        return path.read_text()
    return ""


def sandboxed(app: Path) -> bool:
    signed = subprocess.run(
        ["/usr/bin/codesign", "-d", "--entitlements", "-", str(app)],
        capture_output=True,
        text=True,
    )
    return "com.apple.security.app-sandbox" in signed.stdout + signed.stderr


def home_for(app: Path) -> str:
    """The HOME directory in the environment of the exported game.

    With App Sandbox, HOME points into the container. In the plan we still
    write $HOME, because that is the path before the redirection.
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


def data_dir_of(app: Path) -> Path | None:
    """The per-game storage, as we compute it in the launcher."""
    found = DATA_DIR.search(plan_text(app))
    if not found:
        return None
    return Path(found.group(1).replace("$HOME", home_for(app)))


def log_of(app: Path) -> Path | None:
    """The file to which we send the player's output in the launcher."""
    data = data_dir_of(app)
    if data is None:
        return None
    return data / "logs/launch.log"


def take(app: Path, name: str, script: list[str], output: Path,
         config: dict | None = None) -> str:
    """Run the game to a state and screenshot it. Empty string on success."""
    target = output / f"{name}.png"
    target.unlink(missing_ok=True)
    log = log_of(app)
    if log and log.exists():
        log.unlink()

    # We start each shot from the game as it is shipped. Choosing a pad in the
    # picker writes it to the per-game override, so without this step the shot
    # of the six-button pad would change the pictures of every later shot.
    data = data_dir_of(app)
    if data:
        (data / "controls.cfg").unlink(missing_ok=True)
        for remap in (data / "remaps").rglob("*.rmp"):
            remap.unlink()
        # We save the position of each switch, so otherwise the shot that turns on
        # achievement mode would change the pictures of every later shot.
        for switch in data.glob("toggle-*"):
            switch.unlink()

    # Settings required for this shot, which we add to the per-game override
    # that is already part of the exported launch. The important one is
    # `pause_nonactive`. By default the emulated console stops whenever its
    # window does not have the focus, and we never focus a window here, so
    # otherwise a picture of anything drawn over a running game would show a
    # black game.
    if data and config:
        data.mkdir(parents=True, exist_ok=True)
        (data / "controls.cfg").write_text(
            "".join(f'{key} = "{value}"\n' for key, value in config.items())
        )

    result = subprocess.run(
        [str(launcher_of(app))],
        capture_output=True,
        text=True,
        timeout=TIMEOUT_SECONDS,
        env=dict(
            os.environ,
            ROMINABOX_MENU_SCRIPT=",".join(script),
            ROMINABOX_MENU_SHOT=str(target),
        ),
    )
    written = log.read_text() if log and log.exists() else result.stderr

    # We read this from the player report. Otherwise, when no click in the
    # script happened, we would take a picture of whatever was on screen.
    for line in written.splitlines():
        if "names no element" in line:
            return line.split("[RIB]")[-1].strip()
    if not target.exists():
        tail = "\n".join(written.strip().splitlines()[-6:])
        return f"no screenshot was written (exit {result.returncode})\n{tail}"
    return ""


KIT = ROOT / "desktop/src-tauri/resources/runtime"
# We build it here and check that it comes from this checkout, because every
# worktree shares one cargo target, so the binary next to the manifest may be
# out of date or from another checkout. See scripts/built.py.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from built import cli as _cli  # noqa: E402

CLI = _cli()


def built_player() -> Path | None:
    """The most recent player built in this checkout, if there is one.

    A shot must show the fork as it is now, not as it was when we froze the
    kit, and after a change to the player we build it in this checkout. Each
    checkout has a separate build directory, so in a worktree where we built
    the player, the shots show that player.
    """
    builds = sorted(
        (ROOT / "work").glob("fork-build-*/retroarch/retroarch"),
        key=lambda entry: entry.stat().st_mtime,
    )
    return builds[-1] if builds else None


def build_a_game(
    rom: Path,
    workspace: Path,
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

    The exported game has a separate isolation prefix, so two checkouts
    taking pictures at the same time never share saves or a build.
    """
    settings = dict(settings or {})
    # In a shot, or in the palette loop, we can set the theme and the palette
    # like any other export setting. We copy the package for that export.
    design = str(settings.get("theme", design))
    palette = str(settings.get("palette", palette))
    kit = workspace / "kit"
    shutil.rmtree(kit, ignore_errors=True)
    shutil.copytree(KIT, kit, symlinks=True)

    # Copy the design as it is in this tree, not as when we froze the kit.
    # We read designs/<id> in the exporter. We also copy Native into menu-assets,
    # because in one shot path we read the controller art from there.
    package = ROOT / "integrations/designs" / design
    if not package.is_dir():
        raise SystemExit(f"no design package at {package}")
    staged_design = kit / "designs" / design
    staged_design.mkdir(parents=True, exist_ok=True)
    for document in package.iterdir():
        if document.is_file():
            shutil.copyfile(document, staged_design / document.name)
            if design == "native":
                shutil.copyfile(document, kit / "menu-assets" / document.name)
    player = built_player()
    if player:
        shutil.copyfile(player, kit / "bin/retroarch")
        (kit / "bin/retroarch").chmod(0o755)

    out = workspace / "exported"
    shutil.rmtree(out, ignore_errors=True)
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
        "outputDir": str(out),
        "target": "macos",
        "runtimeKit": str(kit),
    }
    request.update(settings)
    result = subprocess.run(
        [str(CLI), "export"],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        env=dict(os.environ, ROMINABOX_GAME_BUNDLE_PREFIX=workspace.name),
    )
    if result.returncode != 0:
        raise SystemExit(f"could not export a game to shoot:\n{result.stdout[-900:]}")
    app = next(out.glob("*.app"), None)
    if app is None:
        raise SystemExit(f"the export wrote no .app into {out}")
    # The player next to the launcher comes from the kit. Replace it with the
    # freshly built one so that the shot shows this tree.
    if player:
        retroarch = app / "Contents/MacOS/retroarch"
        shutil.copyfile(player, retroarch)
        retroarch.chmod(0o755)
        injector = workspace / "inject-dylib"
        subprocess.run(
            ["cc", "-Oz", "-o", str(injector), str(ROOT / "scripts/native_runtime/inject_dylib.c")],
            check=True,
        )
        subprocess.run(
            [str(injector), str(retroarch), "@executable_path/librominabox-launch.dylib"],
            check=True,
        )
        library = app / "Contents/MacOS/librominabox-launch.dylib"
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(library)], check=True)
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(retroarch)], check=True)
        subprocess.run(
            ["/usr/bin/codesign", "--force", "--sign", "-", "--preserve-metadata=entitlements", str(app)],
            check=True,
        )
    return app


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
    parser.add_argument(
        "--achievements",
        type=Path,
        help="a directory holding achievements.json and badges/, as written by "
             "`rominabox-cli achievements`; without it the game has no achievements screen",
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
    # This applies to the whole run and not to one shot. We fetch the data once
    # and include it in every export of this game.
    achievements = achievement_request(arguments.achievements)
    package = ROOT / "integrations/designs" / arguments.design
    if not package.is_dir():
        raise SystemExit(f"no design package at {package}")
    if not arguments.app:
        if not arguments.rom:
            raise SystemExit("give --app an exported game, or --rom to export one first")
    elif not arguments.app.exists():
        raise SystemExit(f"no exported game at {arguments.app}")
    shutil.rmtree(arguments.output, ignore_errors=True)
    arguments.output.mkdir(parents=True, exist_ok=True)

    # We make one export for each set of export settings, not one per shot,
    # because the build is slow and most shots use the same game.
    exported: dict[str, Path] = {}

    def game_for(settings: dict) -> Path:
        if not arguments.rom:
            return arguments.app
        key = json.dumps(settings, sort_keys=True)
        if key not in exported:
            # A separate directory for each export, because we delete the first
            # game when we build a second one in the same directory, and a
            # separate name, so that two exports have separate saves and bundle ids.
            workspace = ROOT / "work" / f"menu-shots-build-{len(exported)}"
            workspace.mkdir(parents=True, exist_ok=True)
            exported[key] = build_a_game(
                arguments.rom, workspace, arguments.system, settings
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
    for palette in palettes:
        destination = arguments.output / palette if nested else arguments.output
        destination.mkdir(parents=True, exist_ok=True)
        for name, shot in shots.items():
            script = shot["script"]
            config = shot.get("config")
            # The palette and the achievement list are export settings, so we
            # add them to the export key with the settings of the shot.
            settings = {
                key: value
                for key, value in shot.items()
                if key not in ("script", "config")
            }
            settings["palette"] = palette
            settings["theme"] = arguments.design
            if achievements:
                settings["achievements"] = achievements
            key = f"{palette}/{name}" if nested else name
            problem = take(game_for(settings), name, script, destination, config)
            if problem:
                print(f"  FAILED  {key}: {problem}", file=sys.stderr)
                failures.append(key)
                continue
            digests[key] = hashlib.sha256(
                (destination / f"{name}.png").read_bytes()
            ).hexdigest()[:16]
            print(f"  {key:<28}{' -> '.join(script) or '(the menu as it opens)'}")

    # A shot we could not take and a shot that changed are two different
    # results. We report both, so that in one run the shots we could not take
    # do not hide the pictures that changed.
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
        DIGESTS.write_text(json.dumps(digests, indent=2, sort_keys=True) + "\n")
        print(f"\nrecorded {len(digests)} shots -> {DIGESTS.name}")
        return 0

    if arguments.check:
        if not DIGESTS.exists():
            raise SystemExit(f"no recorded shots at {DIGESTS}; run --record first")
        expected = json.loads(DIGESTS.read_text())
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
