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
import shutil
import subprocess
import sys
import tempfile
from contextlib import ExitStack, contextmanager
from pathlib import Path
from typing import Iterator

sys.path.insert(0, str(Path(__file__).resolve().parent))
import exported_game  # noqa: E402
import kit_assets  # noqa: E402
import native_build  # noqa: E402
from core_source import host_target  # noqa: E402
from directory_links import redirected  # noqa: E402
from scratch import remove_made, scratch_run  # noqa: E402

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
    data = exported_game.prepared_storage(app)
    if data is not None:
        for folder in (data, data / "logs", data / "remaps"):
            if redirected(folder):
                return f"refusing a link in the game's storage: {folder}"
    log = exported_game.log_of(app)
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

    inside = exported_game.shot_inside(app, target)

    with (
        tempfile.TemporaryFile(mode="w+t") as stdout_capture,
        tempfile.TemporaryFile(mode="w+t") as stderr_capture,
    ):
        player = subprocess.Popen(
            [str(exported_game.launcher_of(app))],
            stdout=stdout_capture,
            stderr=stderr_capture,
            text=True,
            env=dict(
                os.environ,
                **{exported_game.SCRIPT_ENV: ",".join(script)},
                ROMINABOX_MENU_SHOT=str(inside),
                # No sound, and a transparent window. Without this, we would open
                # CoreAudio during a shot and leave a window on the display.
                **{exported_game.quiet_env(): "1"},
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
    exported_game.carry_shot(inside, target)
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


def built_player() -> Path:
    """The player for a launched test: the test player from player_build,
    built from the fork as it is committed now.

    A shot must show the fork as it is now, not as it was when we froze the
    kit. We drive the menu with a script in every launched test, and only a
    test build has the driver, so we never fall back to the player in the
    kit, which is the one we ship in games.
    """
    build = player_build.selected_build()
    if not player_build.runs_scripts(build):
        raise SystemExit(f"{build} has no menu script driver; build it with ROMINABOX_MENU_SCRIPT_BUILD=1")
    return player_build.player_in(build)


def staged_kit(kit: Path, player: Path | None = None) -> Path:
    """A copy of the runtime kit at `kit` with `player` as its player, or
    the kit's player, and the launcher and every asset of a kit from the
    repository (kit_assets.stage) as they are in this tree, not as they were
    when we made the kit."""
    shutil.copytree(KIT, kit, symlinks=True)
    kit_assets.stage(kit)
    if player is not None:
        installed = kit / native_build.kit_file(host_target(), "player")
        shutil.copyfile(player, installed)
        installed.chmod(0o755)
    # After the player, because we attach a macOS kit's launch library to it.
    native_build.install_tree_launcher(kit, native_build.kit_target(host_target()))
    return kit


def _build_a_game(
    rom: Path,
    workspace: Path,
    run_dir: Path,
    system: str = "megadrive",
    settings: dict | None = None,
    design: str = "native",
    palette: str = "blue",
    namespace: str = "",
) -> Path:
    """Export a game from the tree as it is now, and return what we exported:
    the app of a Mac game, or the single program of a Windows game.

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
    # like any other export setting.
    design = str(settings.get("theme", design))
    palette = str(settings.get("palette", palette))
    kit = staged_kit(run_dir / "kit", built_player())

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
        "target": exported_game.PLATFORM,
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
        env=dict(os.environ, ROMINABOX_GAME_BUNDLE_PREFIX=shot_bundle_prefix(workspace) + namespace),
    )
    if result.returncode != 0:
        raise SystemExit(f"could not export a game to shoot:\n{result.stdout[-900:]}")
    written = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    exported = next((Path(event["result"]["appPath"]) for event in written if event.get("type") == "result"), None)
    if exported is None or not exported.exists():
        raise SystemExit(f"the export wrote no game into {out}")
    return exported


_programs: dict[Path, Path] = {}


def program_of(app: Path) -> Path | None:
    """The single program we exported for a Windows game made with build_a_game,
    which a person opens, or None for a game exported as a folder (macOS)."""
    return _programs.get(app)


@contextmanager
def game_folder(exported: Path) -> Iterator[Path]:
    """The folder of the game we exported at `exported`, for the duration of
    the block. When the block ends, however it ends, we remove everything
    from the game (exported_game.opened), unless a player of it that timed
    out may still be running."""
    with exported_game.opened(exported) as (app, keep):
        if exported.is_file():
            _programs[app] = exported
        try:
            yield app
        except PlayerTimeout as error:
            if error.app == app:
                keep()
            raise
        finally:
            _programs.pop(app, None)


@contextmanager
def build_a_game(
    rom: Path,
    workspace: Path,
    system: str = "megadrive",
    settings: dict | None = None,
    design: str = "native",
    palette: str = "blue",
    namespace: str = "",
) -> Iterator[Path]:
    """Keep one generated export, and the files of its game (game_folder),
    only until the end of the block, however it ends.

    `namespace` is the end of the game's bundle prefix, so games with the same
    content made at once (one per design) have separate storage.

    The app may still be in use by a player that timed out. In that case
    only, keep the temporary directory and print its location for inspection.
    """
    run_dir = Path(tempfile.mkdtemp(prefix=f"rominabox-menu-shots-{scratch_run()}-"))
    created = run_dir.lstat()
    keep = False
    try:
        exported = _build_a_game(rom, workspace, run_dir, system, settings, design, palette, namespace)
        with game_folder(exported) as app:
            try:
                yield app
            except PlayerTimeout as error:
                keep = error.app == app
                if keep:
                    print(f"retained timed-out player's export for inspection: {run_dir}", file=sys.stderr)
                raise
    finally:
        if not keep:
            remove_made(run_dir, created)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, help="an exported .app to shoot")
    parser.add_argument(
        "--rom",
        type=Path,
        help="export a game from this ROM first, using the tree as it is now",
    )
    parser.add_argument("--system", default="megadrive", help="the console --rom is for")
    parser.add_argument("--title", help="the exported game's title (default: Shot Subject)")
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
        if arguments.title:
            settings = {**settings, "title": arguments.title}
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
