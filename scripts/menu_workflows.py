"""Record and check the menu of the exported player.

Use an explicit committed native build, with the worktree environment loaded:
    ROMINABOX_TEST_BUILD=/absolute/build python3 scripts/menu_workflows.py --record
    ROMINABOX_TEST_BUILD=/absolute/build python3 scripts/menu_workflows.py

We use menu_shots for export, launch, capture and lifetimes, and keep the
picture, script and checkpoint log of every case. We change expectations
only with --record. Sound, physical capture and focus/fullscreen are manual.
"""

from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

from PIL import Image

import menu_shots as shots
from make_test_rom import make_megadrive_rom

ROOT = shots.ROOT
EXPECTED = ROOT / "scripts/fixtures/menu-workflow-baseline.json"
OUTPUT = ROOT / "work/test-output/menu-workflows"
CHECKPOINT = re.compile(r"\[RIB\] checkpoint (\S+) (\{.*\})")

# Native frames follow the rate of the drawable, and overlays use wall time.
# We chose these waits to fall inside the hold and gap intervals of each
# design, not at a transition. We still mark the notice-leaving picture inMotion.
OVERLAY_WAITS = {
    "native": {"boot-1-logo": 200, "boot-2-game": 900, "boot-3-notice": 2200,
               "boot-4-gone": 6500, "notice-without-logo": 1700,
               "notice-leaving": 4400, "logo-without-notice": 200},
    "disc": {"boot-1-logo": 200, "boot-2-game": 650, "boot-3-notice": 2200,
             "boot-4-gone": 6500, "notice-without-logo": 1700,
             "notice-leaving": 3280, "logo-without-notice": 200},
}

# We take checkpoints between inputs through the C entry_action and the
# document listeners. The final pictures are of settled states. We test the
# capture blinking in the timed headless test and cancel it before a shot.
WORKFLOWS = {
    "pause-keys": ["report:opened", "key:up", "report:grid", "key:right",
                   "report:right", "key:down", "report:row", "key:left", "report:left"],
    "options-keys": ["options", "report:options", "key:left", "report:volume",
                     "key:down", "report:entry", "key:ok", "report:opened", "key:cancel"],
    "controls-keys": ["options", "controls", "report:first", "key:down", "key:right",
                      "report:moved", "key:up", "report:previous", "key:start", "report:reset"],
    "hover-then-key": ["options", "controls", "hover:control-a", "report:hover",
                       "key:left", "report:key", "hover:heading", "wait:2", "report:left"],
    "binds-c": ["options", "controls", "hover:control-a", "wait-ms:1400", "report:popup"],
    "binds-up": ["options", "controls", "hover:control-up", "wait-ms:1400", "report:popup"],
    "binds-start": ["options", "controls", "hover:control-start", "wait-ms:1400", "report:popup"],
    # Keys go to Controls in the player even while the dropdown is open.
    # Record that behavior, then cancel capture before the stable final picture.
    "picker-key-fallthrough": ["options", "controls", "controls-device-current", "report:picker",
                    "key:down", "report:control", "key:ok", "report:capturing",
                               "key:cancel", "report:cancelled"],
    "capture-cancel": ["options", "controls", "control-a", "report:capturing",
                       "key:cancel", "report:cancelled"],
    "capture-timeout": ["options", "controls", "control-a", "report:capturing",
                        "wait-ms:10500", "report:timed-out"],
    "volume-inputs": ["options", "report:before", "key:left", "report:key",
                      "volume-up", "report:arrow", "volume-level@0.375", "report:fraction"],
    "shader-keys": ["options", "shaders", "report:opened", "key:down", "report:row",
                    "key:ok", "report:chosen", "key:cancel", "report:back"],
    "save-load": ["report:empty", "key:start", "wait-ms:500", "report:saved",
                  "load", "wait-ms:500", "report:loaded"],
}


def fixture(directory: Path) -> tuple[Path, dict]:
    """The original cartridge and offline achievements, without account or game data."""
    directory.mkdir(parents=True, exist_ok=True)
    rom = directory / "menu.md"
    rom.write_bytes(make_megadrive_rom())
    achievements = directory / "achievements"
    (achievements / "badges").mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "scripts/fixtures/menu-achievements.json", achievements / "achievements.json")
    for name, colour in (("fixture.png", (255, 220, 40)), ("fixture_lock.png", (90, 100, 120))):
        Image.new("RGB", (24, 24), colour).save(achievements / "badges" / name)
    return rom, shots.achievement_request(achievements)


def reset_fixture(app: Path) -> None:
    """Reset only the menu files of the generated cartridge, never a supplied app.

    We pass in only exports built here from make_megadrive_rom(). Worktree
    isolation must be active. The files in the state directory of the fixture
    are diagnostic saves we can make again, not user content.
    """
    claim_fixture(app)
    data = shots.data_dir_of(app)
    if data is None or not shots.sandboxed(app):
        raise SystemExit("the generated fixture must have sandboxed game storage")
    for name in ("volume.cfg", "shader-choice"):
        path = data / name
        if path.is_symlink():
            raise SystemExit(f"refusing a symlink in fixture storage: {path}")
        path.unlink(missing_ok=True)
    states = data / "states"
    if states.is_symlink():
        raise SystemExit(f"refusing a symlink in fixture storage: {states}")
    if states.is_dir():
        for path in states.iterdir():
            if path.is_file() and not path.is_symlink():
                path.unlink()


def claim_fixture(app: Path) -> None:
    """Claim empty fixture storage once, and refuse unknown menu data already there."""
    data = shots.data_dir_of(app)
    if data is None or data.is_symlink() or not shots.sandboxed(app):
        raise SystemExit("the fixture requires its own sandboxed game storage")
    sandbox = Path(shots.home_for(app))
    if not data.is_relative_to(sandbox):
        raise SystemExit(f"fixture storage is outside its sandbox: {data}")
    for current in (data, *data.parents):
        if current.is_symlink():
            raise SystemExit(f"refusing a symlink in fixture storage: {current}")
    if not data.resolve().is_relative_to(sandbox.resolve()):
        raise SystemExit(f"fixture storage resolves outside its sandbox: {data}")
    marker = data / "menu-workflow-owner"
    owner = f"{ROOT}\n{hashlib.sha256(make_megadrive_rom()).hexdigest()}\n"
    if marker.is_symlink():
        raise SystemExit(f"refusing a symlink ownership marker: {marker}")
    if marker.exists():
        if marker.read_text() != owner:
            raise SystemExit(f"fixture storage has a different owner: {data}")
        return
    existing = [data / name for name in ("volume.cfg", "controls.cfg", "shader-choice")]
    existing += list(data.glob("toggle-*"))
    for name in ("states", "remaps"):
        directory = data / name
        if directory.is_symlink():
            raise SystemExit(f"refusing a symlink in fixture storage: {directory}")
        if directory.is_dir():
            existing += [path for path in directory.rglob("*") if not path.is_dir()]
    unknown = [path for path in existing if path.exists() or path.is_symlink()]
    if unknown:
        raise SystemExit("pre-existing unowned fixture files; no reset performed:\n"
                         + "\n".join(str(path) for path in unknown))
    data.mkdir(parents=True, exist_ok=True)
    marker.write_text(owner)


def persisted(app: Path) -> dict[str, str]:
    data = shots.data_dir_of(app)
    assert data is not None
    paths = [data / "volume.cfg", data / "controls.cfg", data / "shader-choice"]
    paths += sorted(data.glob("toggle-*")) + sorted((data / "remaps").rglob("*.rmp"))
    # Each export has a separate temporary directory. Canonicalize only this
    # exact app prefix, and keep all filenames, relative paths and other bytes
    # as they are. A path into any other app will still differ.
    app_prefix = str(app.resolve()) + "/"
    return {str(path.relative_to(data)): path.read_text().replace(app_prefix, "$APP/")
            for path in paths if path.is_file()}


def capture(app: Path, destination: Path, name: str, script: list[str], *,
            in_motion: bool = False, reset: bool = True, config: dict | None = None) -> dict:
    if reset:
        reset_fixture(app)
    failure = shots.take(app, name, script + ["report:final"], destination, config, reset_settings=reset)
    if failure:
        raise SystemExit(f"{name}: {failure}")
    reports = {label: json.loads(value) for label, value in
               CHECKPOINT.findall((destination / f"{name}.log").read_text())}
    wanted = [step[7:] for step in script + ["report:final"] if step.startswith("report:")]
    if list(reports) != wanted:
        raise SystemExit(f"{name}: expected checkpoints {wanted}, got {list(reports)}")
    picture = destination / f"{name}.png"
    return {
        "script": script, "reports": reports, "files": persisted(app),
        "picture": None if in_motion else hashlib.sha256(picture.read_bytes()).hexdigest(),
        "size": list(Image.open(picture).size),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    arguments = parser.parse_args()
    if not os.environ.get("ROMINABOX_TEST_BUILD"):
        raise SystemExit("select the exact committed player with ROMINABOX_TEST_BUILD")
    if not os.environ.get("ROMINABOX_GAME_BUNDLE_PREFIX", "").startswith("app.rominabox.game.wt-"):
        raise SystemExit("load scripts/worktree.py env before running the generated fixture")
    player = shots.built_player()  # validates build-info against this fork's HEAD
    assert player is not None
    dirty = subprocess.check_output(["git", "-C", str(ROOT / "vendor/retroarch"),
                                     "status", "--porcelain", "--untracked-files=no"], text=True)
    if dirty:
        raise SystemExit("commit the fork and build it before recording/comparing workflows")
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    rom, achievements = fixture(output / "input")
    provenance = json.loads((player.parents[1] / "build-info.json").read_text())
    provenance["binarySha256"] = hashlib.sha256(player.read_bytes()).hexdigest()
    provenance["cliSha256"] = hashlib.sha256(shots.command().read_bytes()).hexdigest()
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    results = {}
    for design in ("native", "disc"):
        with ExitStack() as exports:
            games = {}

            def game(settings: dict) -> Path:
                key = json.dumps(settings, sort_keys=True)
                if key not in games:
                    games[key] = exports.enter_context(shots.build_a_game(rom, output, settings=settings))
                return games[key]

            for palette in shots.declared_palettes():
                destination = output / design / palette
                destination.mkdir(parents=True, exist_ok=True)
                base = {"theme": design, "palette": palette}
                if design == "native":
                    base["achievements"] = achievements
                cases = shots.declared_shots()
                if design == "disc":
                    # In these cases we record the screen as missing, and we
                    # test it in the Disc inheritance cases.
                    cases = {name: case for name, case in cases.items() if not name.startswith("achievement")}
                if palette != "blue":
                    cases = {name: cases[name] for name in ("pause-menu", "controls", "picker-open")}
                for name, case in cases.items():
                    if name in OVERLAY_WAITS[design]:
                        case = case | {"script": [f"wait-ms:{OVERLAY_WAITS[design][name]}"]}
                    settings = base | {k: v for k, v in case.items() if k not in ("script", "inMotion", "config")}
                    result = capture(game(settings), destination, name, case["script"],
                                     in_motion=case.get("inMotion", False), config=case.get("config"))
                    results[f"{design}/{palette}/{name}"] = result
                    print(f"captured {design}/{palette}/{name}", flush=True)
                if palette != "blue":
                    continue
                app = game(base | {"shaders": {"bundled": ["scanlines", "phosphor"], "initial": "phosphor"}})
                for name, script in WORKFLOWS.items():
                    results[f"{design}/{palette}/{name}"] = capture(app, destination, name, script)
                    print(f"captured {design}/{palette}/{name}", flush=True)
                save = ["options", "volume-level@0.25", "controls", "controls-device-current",
                        "controls-device-option-megadrive6", "report:chosen", "controls-back"]
                reopen = ["options", "report:volume", "controls", "report:controller", "controls-back"]
                if design == "native":
                    save += ["achievements", "achievement-mode", "report:guard"]
                    reopen += ["achievements", "report:guard"]
                results[f"{design}/{palette}/persist"] = capture(app, destination, "persist", save)
                results[f"{design}/{palette}/reopen"] = capture(app, destination, "reopen", reopen, reset=False)
    rendered = json.dumps(results, indent=2, sort_keys=True) + "\n"
    (output / "results.json").write_text(rendered)
    if arguments.record:
        EXPECTED.write_text(rendered)
        print(f"recorded {len(results)} cases; inspect the pictures before accepting this baseline")
        return 0
    expected = json.loads(EXPECTED.read_text())
    changed = [name for name in sorted(results.keys() | expected.keys()) if results.get(name) != expected.get(name)]
    for name in changed:
        before, after = expected.get(name, {}), results.get(name, {})
        fields = [key for key in sorted(before.keys() | after.keys()) if before.get(key) != after.get(key)]
        print(f"CHANGED {name}: {', '.join(fields)}; inspect its log and picture in {output}")
    if changed:
        return 1
    print(f"{len(results)} native menu cases match the recorded baseline")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
