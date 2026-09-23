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
import stat
import subprocess
import sys
import tempfile
from contextlib import ExitStack, contextmanager
from pathlib import Path
from typing import Iterator

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
# The switch is declared in the launcher, and we read its spelling from
# there instead of keeping a second copy. We use the same variable in Cocoa.
QUIET_ENV = re.compile(r'#define ROMINABOX_QUIET_ENV "([A-Z0-9_]+)"')


def quiet_env() -> str:
    found = QUIET_ENV.search(
        (ROOT / "desktop/src-tauri/launcher/main.c").read_text()
    )
    if not found:
        raise SystemExit("launcher does not declare ROMINABOX_QUIET_ENV")
    return found.group(1)


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
         config: dict | None = None, *, reset_settings: bool = True) -> str:
    """Run the game to a state and screenshot it. Empty string on success."""
    # An absolute path, because we tell the game where to write and its working
    # directory is not this one. With a relative output directory, we get the
    # error "failed to open file for writing" for every shot.
    target = output.resolve() / f"{name}.png"
    target.unlink(missing_ok=True)
    log = log_of(app)
    if log and log.exists():
        log.unlink()

    # We start each shot from the game as it is shipped. Choosing a pad in the
    # picker writes it to the per-game override, so without this step the shot
    # of the six-button pad would change the pictures of every later shot.
    data = data_dir_of(app)
    if data and reset_settings:
        (data / "controls.cfg").unlink(missing_ok=True)
        for remap in (data / "remaps").rglob("*.rmp"):
            remap.unlink()
        # We save the position of each switch, so otherwise the shot that turns on
        # achievement mode would change the pictures of every later shot.
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
            "".join(f'{key} = "{value}"\n' for key, value in config.items())
        )

    # In a sandbox, writing is allowed only inside the game's container, so we
    # cannot have the picture written into this repository. We have it written
    # into the game's storage and copy it out here. With a path in the tree, we
    # would get "failed to open file for writing" for every shot.
    inside = target
    if data is not None and sandboxed(app):
        inside = data / "shots" / f"{name}.png"
        inside.parent.mkdir(parents=True, exist_ok=True)
        inside.unlink(missing_ok=True)

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
            log_tail = log.read_text(errors="replace")[-800:] if log and log.exists() else ""
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
    if inside != target and inside.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(inside), str(target))
    written = log.read_text() if log and log.exists() else stderr
    # Copy the evidence next to its picture before we overwrite it next launch.
    (output / f"{name}.log").write_text(written)

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
    text = destination.read_text() if destination.is_file() else ""
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


def built_player() -> Path | None:
    """The most recent player built in this checkout, if there is one.

    A shot must show the fork as it is now, not as it was when we froze the
    kit, and after a change to the player we build it in this checkout. Each
    checkout has a separate build directory, so in a worktree where we built
    the player, the shots show that player.
    """
    selected = os.environ.get("ROMINABOX_TEST_BUILD")
    if selected:
        build = Path(selected).resolve()
        info = json.loads((build / "build-info.json").read_text())
        revision = subprocess.check_output(
            ["git", "-C", str(ROOT / "vendor/retroarch"), "rev-parse", "HEAD"],
            text=True,
        ).strip()
        if info.get("retroarchCommit") != revision:
            raise SystemExit(f"{build} was not built from the current fork commit {revision}")
        player = build / "retroarch/retroarch"
        if not player.is_file():
            raise SystemExit(f"the selected build contains no player: {player}")
        return player
    builds = sorted(
        (ROOT / "work").glob("fork-build-*/retroarch/retroarch"),
        key=lambda entry: entry.stat().st_mtime,
    )
    return builds[-1] if builds else None


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
        "outputDir": str(out),
        "target": "macos",
        "runtimeKit": str(kit),
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
    app = next(out.glob("*.app"), None)
    if app is None:
        raise SystemExit(f"the export wrote no .app into {out}")
    # The player next to the launcher comes from the kit. Replace it with the
    # freshly built one so that the shot shows this tree.
    if player:
        # Do this before the copy. After the copy, the export has no signature.
        capture_export_entitlements(app, run_dir / "entitlements.plist")
        retroarch = app / "Contents/MacOS/retroarch"
        shutil.copyfile(player, retroarch)
        retroarch.chmod(0o755)
        injector = run_dir / "inject-dylib"
        subprocess.run(
            ["cc", "-Oz", "-o", str(injector), str(ROOT / "scripts/native_runtime/inject_dylib.c")],
            check=True,
        )
        subprocess.run(
            [str(injector), str(retroarch), "@executable_path/librominabox-launch.dylib"],
            check=True,
        )
        resign_replaced_player(app, run_dir / "entitlements.plist")
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
                # The palette and the achievement list are export settings, so we
                # add them to the export key with the settings of the shot.
                settings = {
                    key: value
                    for key, value in shot.items()
                    # "inMotion" is about how we compare this shot, not how we
                    # export the game.
                    if key not in ("script", "config", "inMotion")
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
        # We merge instead of replacing. A run with --only has no results for
        # the other shots, and writing only its results would drop theirs.
        kept = json.loads(DIGESTS.read_text()) if DIGESTS.exists() else {}
        kept.update(digests)
        DIGESTS.write_text(json.dumps(kept, indent=2, sort_keys=True) + "\n")
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
