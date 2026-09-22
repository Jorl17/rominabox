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
    python3 scripts/menu_shots.py --rom game.rom --palette <id>
    python3 scripts/menu_shots.py --rom game.rom --every-palette
    python3 scripts/menu_shots.py --app "/path/to/Game.app" --check

We pass `--palette` on to the export, and its default is blue, the palette
of the existing callers. With `--every-palette` we read
`desktop/designs.json` and export each palette into a separate workspace,
writing `<output>/<palette>/<shot>.png`. The keys of the digest are the
palette and then the shot. We reject a digest in the old flat form, with one
hash per shot and no palette, and with `--check` we ask for a new recording.

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

sys.path.insert(0, str(Path(__file__).resolve().parent))
import worktree

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "scripts/fixtures/menu-shots.json"
DIGESTS = ROOT / "scripts/fixtures/menu-shot-digests.json"
DESIGNS = ROOT / "desktop/designs.json"
# The default palette. With `--every-palette` we read the list of palettes
# from desktop/designs.json instead of writing it here.
DEFAULT_PALETTE = "blue"

# A generous limit. The game quits as soon as the picture is written, and we
# use this limit only so that a stuck run cannot stop the tests forever.
TIMEOUT_SECONDS = 120

DATA_DIR = re.compile(r'^data_dir="([^"]+)"', re.MULTILINE)


def declared_shots() -> dict[str, list[str]]:
    """Each named shot and the elements clicked to reach it."""
    return json.loads(SHOTS.read_text())["shots"]


def declared_palettes() -> list[str]:
    """Palette ids from desktop/designs.json, in the order they are declared."""
    names = [entry["id"] for entry in json.loads(DESIGNS.read_text())["palettes"]]
    if not names:
        raise SystemExit(f"{DESIGNS.name} declares no palettes")
    return names


def require_palette(name: str) -> str:
    declared = declared_palettes()
    if name not in declared:
        raise SystemExit(f"unknown palette '{name}'; declared: {', '.join(declared)}")
    return name


def launcher_of(app: Path) -> Path:
    """The launcher in the app, so the game starts as it does for a player."""
    candidates = [entry for entry in (app / "Contents/MacOS").iterdir() if entry.is_file()]
    for entry in candidates:
        if entry.name != "retroarch" and os.access(entry, os.X_OK):
            return entry
    raise SystemExit(f"no launcher inside {app}")


def log_of(app: Path) -> Path | None:
    """The file to which we send the player's output in the launcher."""
    found = DATA_DIR.search(launcher_of(app).read_text())
    if not found:
        return None
    return Path(os.path.expandvars(found.group(1).replace("$HOME", str(Path.home())))) / "logs/launch.log"


def data_dir_of(app: Path) -> Path | None:
    """The per-game storage, as we compute it in the launcher."""
    found = DATA_DIR.search(launcher_of(app).read_text())
    if not found:
        return None
    return Path(found.group(1).replace("$HOME", str(Path.home())))


def take(app: Path, name: str, script: list[str], output: Path) -> str:
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


# Where we put a freshly built player, so that a shot shows the fork as it
# is now and not as it was when we froze the kit.
BUILT_PLAYER = ROOT / "work/fork-build-20260920/retroarch/retroarch"
KIT = ROOT / "desktop/src-tauri/resources/runtime"
DESIGN = ROOT / "integrations/designs/native"
CLI = worktree.cli_path(ROOT)


def build_a_game(
    rom: Path,
    workspace: Path,
    system: str = "megadrive",
    palette: str = DEFAULT_PALETTE,
    achievements: Path | None = None,
) -> Path:
    """Export a game from the tree as it is now, and return the app.

    To take a picture of a change by hand, someone has to assemble a kit,
    remember which pieces are out of date, export, and replace the player
    binary, and each of these four steps can go wrong. The kit is a build
    output, so we refresh it from the tree here instead of trusting it.

    The exported game has a separate isolation prefix, so two checkouts
    taking pictures at the same time never share saves or a build.
    """
    palette = require_palette(palette)
    kit = workspace / "kit"
    shutil.rmtree(kit, ignore_errors=True)
    shutil.copytree(KIT, kit, symlinks=True)

    # Copy the design as it is in this tree, not as when we froze the kit.
    for document in DESIGN.iterdir():
        if document.is_file():
            shutil.copyfile(document, kit / "designs/native" / document.name)
            shutil.copyfile(document, kit / "menu-assets" / document.name)
    if BUILT_PLAYER.exists():
        shutil.copyfile(BUILT_PLAYER, kit / "bin/retroarch")
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
        "theme": "native",
        "palette": palette,
        "menuSounds": "off",
        "splash": False,
        "advancedEmulatorAccess": False,
        # The shader shots require a bundled shader, because without one there is
        # no shader screen in the exported game to click on.
        "shaders": {"bundled": ["scanlines", "phosphor"], "initial": "none"},
        # We fetch these beforehand with `rominabox-cli achievements`, so that a
        # picture does not depend on the network or on who is signed in.
        "achievements": achievement_request(achievements),
        "outputDir": str(out),
        "target": "macos",
        "runtimeKit": str(kit),
    }
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
    if BUILT_PLAYER.exists():
        shutil.copyfile(BUILT_PLAYER, app / "Contents/MacOS/retroarch")
        (app / "Contents/MacOS/retroarch").chmod(0o755)
    return app


def achievement_request(directory: Path | None) -> dict:
    """The achievement settings we pass to the export.

    The list and its badges are third-party material and are not in this
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


def capture(app: Path, destination: Path, palette: str, nested: bool) -> tuple[list[str], dict[str, str]]:
    """Take each declared shot in one game. Return (failed names, digests)."""
    destination.mkdir(parents=True, exist_ok=True)
    failures: list[str] = []
    digests: dict[str, str] = {}
    for name, script in declared_shots().items():
        label = f"{palette}/{name}" if nested else name
        problem = take(app, name, script, destination)
        if problem:
            print(f"  FAILED  {label}: {problem}", file=sys.stderr)
            failures.append(label)
            continue
        digests[name] = hashlib.sha256((destination / f"{name}.png").read_bytes()).hexdigest()[:16]
        reached = " -> ".join(script) or "(the menu as it opens)"
        print(f"  {label:<28}{reached}")
    return failures, digests


def flat_record(record: object) -> bool:
    """The single-palette file: each shot name maps to one hash string."""
    return (
        isinstance(record, dict)
        and bool(record)
        and all(isinstance(value, str) for value in record.values())
    )


def flat_digest_message() -> str:
    return (
        f"{DIGESTS.name} has the old shape: each shot names one hash, and "
        "nothing names a palette. The same shot in two palettes is two "
        "pictures. Re-record after looking at them:\n"
        "  python3 scripts/menu_shots.py --rom <game> --every-palette --record"
    )


def digest_problems(
    actual: dict[str, dict[str, str]],
    expected: object,
    *,
    complete: bool,
) -> list[str]:
    """Mismatches against a palette-keyed record. Empty when it matches."""
    if not isinstance(expected, dict):
        return ["the digest file is not a palette map"]
    problems: list[str] = []
    for palette, shots in actual.items():
        recorded = expected.get(palette)
        if not isinstance(recorded, dict):
            problems.append(f"  MISSING {palette}: not in the recorded digests")
            continue
        for name in sorted(shots):
            if recorded.get(name) != shots[name]:
                problems.append(f"  CHANGED {palette}/{name}")
        for name in sorted(set(recorded) - set(shots)):
            problems.append(f"  MISSING {palette}/{name}: no longer shot")
    if complete:
        for palette in sorted(set(expected) - set(actual)):
            if isinstance(expected[palette], dict):
                problems.append(f"  MISSING {palette}: no longer shot")
    return problems


def write_shot_digests(actual: dict[str, dict[str, str]], complete: bool) -> None:
    """Write a palette-keyed digest. We replace a flat file, which we cannot merge."""
    if complete or not DIGESTS.exists():
        record = actual
    else:
        loaded = json.loads(DIGESTS.read_text())
        if isinstance(loaded, dict) and not flat_record(loaded):
            record = {key: value for key, value in loaded.items() if isinstance(value, dict)}
            record.update(actual)
        else:
            record = actual
    DIGESTS.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, help="an exported .app to shoot")
    parser.add_argument(
        "--rom",
        type=Path,
        help="export a game from this ROM first, using the tree as it is now",
    )
    parser.add_argument("--system", default="megadrive", help="the console --rom is for")
    parser.add_argument(
        "--palette",
        help="export this declared palette (default: blue)",
    )
    parser.add_argument(
        "--every-palette",
        action="store_true",
        help="export and shoot every palette in desktop/designs.json",
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
        else [require_palette(arguments.palette or DEFAULT_PALETTE)]
    )

    apps: dict[str, Path] = {}
    if arguments.every_palette:
        # We empty workspace/exported in build_a_game, so exporting a second
        # palette there would replace the app we still use for the first one.
        for palette in palettes:
            workspace = ROOT / "work/menu-shots-build" / palette
            workspace.mkdir(parents=True, exist_ok=True)
            apps[palette] = build_a_game(
                arguments.rom, workspace, arguments.system, palette, arguments.achievements
            )
            print(f"  built    {palette:<8}{apps[palette].name}")
    else:
        palette = palettes[0]
        if arguments.rom:
            workspace = ROOT / "work/menu-shots-build"
            workspace.mkdir(parents=True, exist_ok=True)
            arguments.app = build_a_game(
                arguments.rom, workspace, arguments.system, palette, arguments.achievements
            )
            print(f"  built    {arguments.app.name}")
        if not arguments.app:
            raise SystemExit("give --app an exported game, or --rom to export one first")
        if not arguments.app.exists():
            raise SystemExit(f"no exported game at {arguments.app}")
        apps[palette] = arguments.app

    shutil.rmtree(arguments.output, ignore_errors=True)
    arguments.output.mkdir(parents=True, exist_ok=True)

    failures: list[str] = []
    digests: dict[str, dict[str, str]] = {}
    for palette, app in apps.items():
        destination = arguments.output / palette if arguments.every_palette else arguments.output
        failed, pictured = capture(app, destination, palette, arguments.every_palette)
        failures.extend(failed)
        digests[palette] = pictured

    if failures:
        print(f"\n{len(failures)} shot(s) failed: {', '.join(failures)}", file=sys.stderr)
        return 1

    if arguments.record:
        write_shot_digests(digests, complete=arguments.every_palette)
        recorded = sum(len(shots) for shots in digests.values())
        print(f"\nrecorded {recorded} shots across {len(digests)} palette(s) -> {DIGESTS.name}")
        return 0

    if arguments.check:
        if not DIGESTS.exists():
            raise SystemExit(f"no recorded shots at {DIGESTS}; run --record first")
        expected = json.loads(DIGESTS.read_text())
        if flat_record(expected):
            print(f"\n{flat_digest_message()}", file=sys.stderr)
            return 1
        problems = digest_problems(
            digests, expected, complete=arguments.every_palette
        )
        if problems:
            for line in problems:
                print(line, file=sys.stderr)
            print(
                "\nThe game draws a menu state differently. Look at the pictures "
                "before re-recording:\n"
                "  python3 scripts/menu_shots.py --rom <game> --every-palette --record",
                file=sys.stderr,
            )
            return 1
        recorded = sum(len(shots) for shots in digests.values())
        print(f"\n{len(digests)} palette(s), {recorded} shots unchanged")
        return 0

    recorded = sum(len(shots) for shots in digests.values())
    print(f"\n{recorded} shots across {len(digests)} palette(s) -> {arguments.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
