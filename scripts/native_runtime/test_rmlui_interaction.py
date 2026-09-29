"""Headless RmlUi interaction checks, with no RetroArch and no window.

In menu_harness.py we compile the menu sources that Makefile.common lists,
cache them per object, and link them into test programs. The binaries stay
in this checkout, so one checkout never runs the tests of another. Every path
that we pass to a program is a native path, so the same run works on macOS
and on Windows.
"""

from __future__ import annotations

import functools
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SCRIPTS))
from scratch import scratch  # noqa: E402

ROOT = SCRIPTS.parent
HERE = SCRIPTS / "native_runtime"
PYTHON = sys.executable
DESIGNS = ROOT / "integrations/designs"
BUILD = ROOT / "work/bridge-interaction"
# The stylesheet that we write in an export, not the file of the design. A
# design has design(surface) where a colour goes, and we fill it in at export,
# so the file of the design alone gives RmlUi tokens that it cannot parse.
ASSETS = BUILD / "assets"
# We write fixtures byte for byte as the programs read them, with LF line
# ends on every platform.
INTERACTION = BUILD / "test_rmlui_interaction"
ORCHESTRATION = BUILD / "test_menu_orchestration"
MENU_CONTROLS = BUILD / "test_menu_controls"
# A game folder whose name has the characters that we escape in the menu
# before RmlUi gets a path, as a title can have on each platform.
NAMED_FOLDERS = {"darwin": "Who Wants a Game?", "linux": "Who Wants a Game?", "win32": "100% Game"}


def harness(output: Path, *sources: Path) -> None:
    subprocess.run([PYTHON, str(HERE / "menu_harness.py"), "build", str(output),
                    "--define", "HAVE_AUDIOMIXER", *map(str, sources)], check=True)


@functools.cache
def cli() -> str:
    # We build it here and check that it comes from this checkout. When
    # worktrees share one cargo target, the binary next to the manifest may be
    # stale or from another checkout. See scripts/built.py.
    found = subprocess.check_output([PYTHON, str(SCRIPTS / "built.py")], text=True).strip()
    if not Path(found).is_file():
        raise SystemExit("build the CLI first: cargo build --release --bin rominabox-cli")
    return found


def ask(command: str, request: dict, **options) -> subprocess.CompletedProcess:
    return subprocess.run([cli(), command], input=json.dumps(request), text=True, check=True, **options)


def stage_theme(design: Path, destination: Path) -> None:
    ask("stage-theme", {"source": str(design), "destination": str(destination), "palette": "blue"},
        stdout=subprocess.DEVNULL)


def list_fixture(menu: Path, *flags: str) -> None:
    # A generated list screen for the bridge (list_fixture.py). Without it the
    # bridge has no list, and the checks of the rows and controls on a list
    # screen cannot fail.
    subprocess.run([PYTHON, str(HERE / "list_fixture.py"), str(menu), *flags], check=True)


def placements() -> bool:
    """Bind lists and the volume thumb, for every controller declared in the
    repository and every design. We request the scenes from the exporter and
    measure the boxes that the bridge lays out."""
    designs = [entry["id"] for entry in json.loads((ROOT / "desktop/designs.json").read_text())["designs"]]
    profiles = json.loads((ROOT / "desktop/controls.json").read_text())["profiles"]
    controllers = ROOT / "desktop/assets/controllers"
    failed = False
    for design in designs:
        design_dir = DESIGNS / design
        assets = BUILD / f"placement-{design}"
        scenes = assets / "scenes"
        scenes.mkdir(parents=True, exist_ok=True)
        stage_theme(design_dir, assets)
        # What we read in the player, from the design or inherited from Native.
        staged = dict(re.findall(r'^(\w+) = "(.*)"$', (assets / "design.cfg").read_text(), re.M))
        after, width = int(staged["binds_after"]), int(staged["binds_width"])
        if after != 1200:
            print(f"FAIL {design} binds_after is {after}; the list should wait 1200 ms", file=sys.stderr)
            failed = True
        for profile in profiles:
            system = profile["systems"][0] if profile["systems"] else "megadrive"
            dest = assets / "stage" / profile["id"]
            dest.mkdir(parents=True, exist_ok=True)
            ask("stage-controls", {
                "system": system,
                "source": str(controllers),
                "design": str(design_dir),
                "destination": str(dest),
                "controls": {"profile": profile["id"]},
            }, stdout=subprocess.DEVNULL)
            scene = dest / f"scene-{profile['id']}.rml"
            if not scene.is_file():
                print(f"FAIL {design}/{profile['id']}: exporter wrote no scene", file=sys.stderr)
                failed = True
                continue
            (scenes / f"{profile['id']}.rml").write_text(scene.read_text(), newline="\n")
            # Where the exporter's layout puts every leader run and ring, and
            # their size, by stop, in the order of scene-geometry. A pad with
            # no drawing has none.
            marks = []
            if profile.get("image"):
                asked = ask("scene-geometry", {"system": system, "profile": profile["id"],
                                               "design": str(design_dir)}, capture_output=True)
                layout = json.loads(asked.stdout)["result"]
                for placed in layout["controls"]:
                    for mark in [*placed["leader"], placed["marker"]]:
                        marks.append(f"control-{placed['id']}\t{mark['x']}\t{mark['y']}\t{mark['width']}\t{mark['height']}")
                for group in layout["groups"]:
                    if group["marker"]:
                        for mark in [*group["leader"], group["marker"]]:
                            marks.append(f"control-group-{group['name']}\t{mark['x']}\t{mark['y']}\t{mark['width']}\t{mark['height']}")
            (scenes / f"{profile['id']}.marks").write_text("".join(line + "\n" for line in marks), newline="\n")
            lines = []
            for control in profile["controls"]:
                group = control.get("group") or ""
                anchor = f"control-group-{group}" if group else f"control-{control['id']}"
                title = control["label"]
                lines.append(f"{anchor}\t{title}\t{control['key']}\tKEY")
                control_id = control["id"]
                if control_id in ("up", "down", "left", "right"):
                    detail, kind = f"Hat #0 {title}", "PAD"
                elif group.endswith("stick") and not control_id.endswith("3"):
                    detail, kind = "Axis -0", "AXIS"
                else:
                    detail, kind = "Button 0", "PAD"
                lines.append(f"{anchor}\t{title}\t{detail}\t{kind}")
            (scenes / f"{profile['id']}.lines").write_text("\n".join(lines) + "\n", newline="\n")
        # We measure the list in the document of one export, which has the
        # bind rows and the volume control, and swap in the scenes above.
        menu = assets / "stage" / profiles[0]["id"] / "menu.rml"
        if not menu.is_file():
            print(f"FAIL {design}: exporter wrote no menu", file=sys.stderr)
            failed = True
            continue
        (assets / "menu.rml").write_bytes(menu.read_bytes())
        if subprocess.run([str(INTERACTION), str(assets), "placement", str(scenes), design, str(width)]).returncode != 0:
            failed = True
    return not failed


def orchestration_fixtures() -> None:
    """The staged Native export for the orchestration cases.

    We replace only the RetroArch host commands. Config parsing, file writes,
    declarations and the RmlUi document are the production code. All eight
    declared Mega Drive callouts are active, so we can move through one scene
    by pointer and keyboard. They are fixture bindings, not a runtime remap."""
    assets = BUILD / "placement-native"
    ids = ("up", "left", "right", "down", "y", "b", "a", "start")
    (assets / "controls-defaults.cfg").write_text(''.join(
        f'rib_label_{id} = "{id}"\ninput_player1_{id} = "a"\n' for id in ids
    ), newline="\n")

    menu = assets / "menu.rml"
    markup = menu.read_text()
    anchor = '<div id="unlock-row"'
    assert anchor in markup
    assert '<button class="menu-action options-back"' in markup
    markup = markup.replace(
        anchor,
        '<div id="fixture-panel" class="screen-panel" '
        'style="display:none;position:absolute;left:100dp;top:100dp;width:600dp;height:300dp;">'
        '<div class="list" style="width:500dp;" data-page-size="2"><div class="list-page">'
        '<button id="fixture-one" class="list-row" style="width:400dp;height:42dp;">ONE</button>'
        '<button id="fixture-two" class="list-row" style="width:400dp;height:42dp;">TWO</button>'
        '</div></div><button id="fixture-back" class="menu-action list-back">BACK</button>'
        '</div>' + anchor,
        1,
    )
    markup = markup.replace(
        '<button class="menu-action options-back"',
        '<button id="fixture" class="menu-action option-entry">TEST LIST</button>'
        '<button class="menu-action options-back"',
        1,
    )
    menu.write_text(markup, newline="\n")
    declarations = assets / "design.cfg"
    config = declarations.read_text()
    # We add the screen of the fixture to the screens that the design
    # declares, which end with this platform's screen (UNINSTALL or RESET).
    declared = re.search(r'^screens = "pause options controls menu-controls[^"]*"$', config, re.M)
    assert declared, config
    assert 'screen_button_options = "options"' in config
    config = config.replace(declared.group(0), declared.group(0)[:-1] + ' fixture"', 1)
    config = config.replace('screen_button_options = "options"',
                            'screen_button_options = "options fixture-back"', 1)
    config += ('\nscreen_panel_fixture = "fixture-panel"'
               '\nscreen_heading_fixture = "TEST LIST"'
               '\nscreen_footer_fixture = "ESC BACK"'
               '\nscreen_button_fixture = "fixture"\n')
    declarations.write_text(config, newline="\n")

    profiles = json.loads((ROOT / "desktop/controls.json").read_text())["profiles"]
    assert max(len(profile["controls"]) for profile in profiles) <= 48, "declared profile exceeds the preserved player limit"
    profile = next(profile for profile in profiles if profile["id"] == "ps1-analog")
    ids = [control["id"] for control in profile["controls"]]
    assert len(ids) == 24
    ids += [f"extra{index:02d}" for index in range(25)]
    defaults = assets / "stage" / "ps1-analog" / "controls-defaults.cfg"
    (defaults.parent / "expected-controls.txt").write_text("\n".join(ids[:48]) + "\n", newline="\n")
    for support in ("menu.rcss", "Silkscreen-Regular.ttf"):
        (defaults.parent / support).write_bytes((assets / support).read_bytes())
    groups = {control["id"]: control.get("group") for control in profile["controls"]}

    def pad_defaults(ids):
        return (
            'controls_profile = "ps1-analog"\n'
            + 'controls_variant_controls_ps1-analog = "' + ' '.join(ids) + '"\n'
            + ''.join(f'rib_label_{id} = "{id}"\ninput_player1_{id} = "a"\n' for id in ids)
            + ''.join(f'rib_group_{id} = "{group}"\n' for id, group in groups.items() if group)
        )
    defaults.write_text(pad_defaults(ids), newline="\n")
    # The same pad with only its 24 controls, in every other design, for the
    # cases that use an actual pad with sticks instead of an overfull one.
    for other in BUILD.glob("placement-*/stage/ps1-analog"):
        if other != defaults.parent:
            (other / "controls-defaults.cfg").write_text(pad_defaults(ids[:24]), newline="\n")


def stage_everything() -> None:
    """A whole Native menu as in a game with filters and achievements,
    staged by the exporter, for the cases that choose rows on generated list
    screens."""
    assets = BUILD / "placement-everything"
    design = DESIGNS / "native"
    assets.mkdir(parents=True, exist_ok=True)
    stage_theme(design, assets)
    ask("stage-controls", {
        "system": "megadrive", "source": str(ROOT / "desktop/assets/controllers"),
        "design": str(design), "destination": str(assets), "palette": "blue",
        "includeAchievements": True, "shaders": {"bundled": ["scanlines", "phosphor"]},
    }, stdout=subprocess.DEVNULL)


def orchestrate() -> None:
    # A failing workflow may save a configuration before it reports the
    # failure. With the project's scratch context, every run has fixed inputs,
    # in folders with non-ASCII names like a player's, which we pass to the
    # program as arguments.
    with scratch("rominabox-menu-orchestration-João-") as data:
        subprocess.run([str(ORCHESTRATION), str(BUILD / "placement-native"), data], check=True)
    with scratch("rominabox-menu-capacity-João-") as data:
        subprocess.run([str(ORCHESTRATION), "--capacity",
                        str(BUILD / "placement-native/stage/ps1-analog"), data], check=True)
    # A stick waiting to be rebound, on the PlayStation analogue pad, in
    # every design.
    failed = []
    for staged in sorted(BUILD.glob("placement-*/stage/ps1-analog")):
        with scratch("rominabox-menu-stick-capture-João-") as data:
            if subprocess.run([str(ORCHESTRATION), "--stick-capture", str(staged), data]).returncode != 0:
                failed.append(staged.parent.parent.name)
    if failed:
        raise SystemExit(f"stick capture failed in {', '.join(failed)}")


def menu_controls() -> bool:
    """MENU CONTROLS in every design: each staged twice by the exporter, with
    the builder's defaults and with other ones, as a later export of the same
    game would be, and run with a separate data folder."""
    designs = [entry["id"] for entry in json.loads((ROOT / "desktop/designs.json").read_text())["designs"]]
    failed = []
    for design in designs:
        staged = {}
        for name, controls in (("first", None), ("later", {"confirm": ["key:space", "pad:x"]})):
            assets = BUILD / f"menu-controls-{design}-{name}"
            assets.mkdir(parents=True, exist_ok=True)
            ask("stage-controls", {
                "system": "megadrive", "source": str(ROOT / "desktop/assets/controllers"),
                "design": str(DESIGNS / design), "destination": str(assets), "palette": "blue",
                **({"menuControls": controls} if controls else {}),
            }, stdout=subprocess.DEVNULL)
            staged[name] = assets
        print(f"menu controls {design}", flush=True)
        with scratch("rominabox-menu-controls-João-") as data:
            if subprocess.run([str(MENU_CONTROLS), str(staged["first"]), str(staged["later"]), data]).returncode != 0:
                failed.append(design)
    if failed:
        print(f"FAIL menu controls in {', '.join(failed)}", file=sys.stderr)
    return not failed


def main() -> int:
    drivers = ROOT / "vendor/retroarch/menu/drivers"
    if not (drivers / "rmlui/view.cpp").is_file():
        raise SystemExit(f"missing RmlUi menu view at {drivers}")
    BUILD.mkdir(parents=True, exist_ok=True)
    harness(INTERACTION, HERE / "test_rmlui_interaction.cpp", HERE / "test_menu_declarations.cpp",
            HERE / "test_menu_folder.cpp", HERE / "menu_host_fake.cpp")
    subprocess.run([str(INTERACTION), "declarations"], check=True)

    ASSETS.mkdir(parents=True, exist_ok=True)
    for document in (DESIGNS / "native").iterdir():
        if document.is_file():
            shutil.copy(document, ASSETS / document.name)
    try:
        stage_theme(DESIGNS / "native", ASSETS)
    except subprocess.CalledProcessError:
        raise SystemExit("could not stage the design's stylesheet")
    list_fixture(ASSETS / "menu.rml", "--actions")

    # A slot's picture in a folder with a non-ASCII name, like a player's data
    # folder, which we read in the menu through the libretro file layer.
    (BUILD / "João").mkdir(parents=True, exist_ok=True)
    subprocess.run([str(INTERACTION), str(ASSETS), str(BUILD / "João/thumbnail-test.png")], check=True)

    # A game folder named like a title. RmlUi keeps the path of a document as
    # a URL, which ends at a "?", so in the menu we escape "?" and "%", the
    # escape character. No Windows file name can contain "?" (at export we
    # write it as "-"), so there the folder name has the "%".
    folder_name = NAMED_FOLDERS.get(sys.platform)
    if folder_name is None:
        raise SystemExit(f"no folder name with the menu's escaped characters is declared for {sys.platform}")
    named = BUILD / folder_name / "menu-assets"
    named.mkdir(parents=True, exist_ok=True)
    for document in (DESIGNS / "native").iterdir():
        if document.is_file():
            shutil.copy(document, named / document.name)
    stage_theme(DESIGNS / "native", named)
    print(f"styled under a folder named {folder_name!r}", flush=True)
    styled_ok = subprocess.run([str(INTERACTION), str(named), "named-folder"]).returncode == 0

    # The same rows under every design's stylesheet. Native keeps a constant
    # border, and the disc accent is present on every row, so focus does not
    # move the name of a row to the right.
    row_edges_ok = True
    for edge_design in ("native", "disc"):
        edge_assets = BUILD / f"row-edge-{edge_design}"
        edge_assets.mkdir(parents=True, exist_ok=True)
        stage_theme(DESIGNS / edge_design, edge_assets)
        list_fixture(edge_assets / "menu.rml")
        print(f"row-edge {edge_design}", flush=True)
        if subprocess.run([str(INTERACTION), str(edge_assets), "row-edge"]).returncode != 0:
            row_edges_ok = False

    if not placements():
        return 1
    orchestration_fixtures()
    harness(ORCHESTRATION, HERE / "test_menu_orchestration.cpp", HERE / "menu_host_fake.cpp",
            HERE / "text_test_host.cpp", ROOT / "vendor/retroarch/libretro-common/file/config_file.c")
    stage_everything()
    orchestrate()
    harness(MENU_CONTROLS, HERE / "test_menu_controls.cpp", HERE / "menu_host_fake.cpp",
            HERE / "text_test_host.cpp", ROOT / "vendor/retroarch/libretro-common/file/config_file.c")
    controls_ok = menu_controls()
    return 0 if row_edges_ok and controls_ok and styled_ok else 1


if __name__ == "__main__":
    sys.exit(main())
