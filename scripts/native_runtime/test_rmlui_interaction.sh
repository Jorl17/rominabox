#!/bin/sh
# Headless RmlUi interaction checks, with no RetroArch and no window.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bridge_dir=$repo_root/vendor/retroarch/menu/drivers
# Compile the menu sources that Makefile.common lists, cached per object, and
# link a program against them. The binary stays in this checkout, so one
# checkout never runs the tests of another.
harness() { python3 "$script_dir/menu_harness.py" build "$@"; }
design=$repo_root/integrations/designs/native
build_dir=$repo_root/work/bridge-interaction
# The stylesheet that we write in an export, not the file of the design. A
# design has design(surface) where a colour goes, and we fill it in at export,
# so the file of the design alone gives RmlUi tokens that it cannot parse.
assets=$build_dir/assets
out=$build_dir/test_rmlui_interaction

if [ ! -f "$bridge_dir/rmlui/view.cpp" ]; then
  echo "missing RmlUi menu view at $bridge_dir" >&2
  exit 1
fi

mkdir -p "$build_dir"
harness "$out" --define HAVE_AUDIOMIXER \
  "$script_dir/test_rmlui_interaction.cpp" "$script_dir/test_menu_declarations.cpp"

"$out" declarations

mkdir -p "$assets"
for document in "$design"/*; do
  [ -f "$document" ] && cp "$document" "$assets/"
done
# We build it here and check that it comes from this checkout, because every
# worktree shares one cargo target, so the binary next to the manifest may be
# out of date or from another checkout. See scripts/built.py.
cli=$(python3 "$repo_root/scripts/built.py")
if [ -x "$cli" ]; then
  printf '{"source":"%s","destination":"%s","palette":"blue"}' "$design" "$assets" \
    | "$cli" stage-theme >/dev/null || {
      echo "could not stage the design's stylesheet" >&2; exit 1; }
else
  echo "build the CLI first: cargo build --release --bin rominabox-cli" >&2
  exit 1
fi

# A generated list screen for the bridge (list_fixture.py). Without it the
# bridge has no list, and the checks of the rows and controls on a list
# screen cannot fail.
python3 "$script_dir/list_fixture.py" "$assets/menu.rml" --actions

"$out" "$assets" "$build_dir/thumbnail-test.png"

# The same rows under every design's stylesheet. Native keeps a constant
# border, and the disc accent is present on every row, so focus does not
# move the name of a row to the right.
row_edge_failed=0
for edge_design in native disc; do
  edge_assets="$build_dir/row-edge-$edge_design"
  mkdir -p "$edge_assets"
  printf '{"source":"%s","destination":"%s","palette":"blue"}' \
    "$repo_root/integrations/designs/$edge_design" "$edge_assets" \
    | "$cli" stage-theme >/dev/null
  python3 "$script_dir/list_fixture.py" "$edge_assets/menu.rml"
  echo "row-edge $edge_design"
  "$out" "$edge_assets" row-edge || row_edge_failed=1
done

# Bind lists and the volume thumb, for every controller declared in the
# repository and both designs. We request the scenes from the exporter and
# measure the boxes that the bridge lays out.
python3 - "$repo_root" "$build_dir" "$out" <<'PY'
import json, subprocess, sys
from pathlib import Path

root = Path(sys.argv[1])
build = Path(sys.argv[2])
binary = sys.argv[3]
designs = [entry["id"] for entry in json.loads((root / "desktop/designs.json").read_text())["designs"]]
profiles = json.loads((root / "desktop/controls.json").read_text())["profiles"]
controllers = root / "desktop/assets/controllers"
cli = subprocess.check_output(["python3", str(root / "scripts/built.py")], text=True).strip()
failed = False

for design in designs:
    declared = json.loads((root / "integrations/designs" / design / "design.json").read_text())
    after = declared["binds"]["afterMs"]
    width = declared["binds"]["width"]
    if after != 1200:
        print(f"FAIL {design} binds.afterMs is {after}; the list should wait 1200 ms", file=sys.stderr)
        failed = True
    design_dir = root / "integrations/designs" / design
    assets = build / f"placement-{design}"
    scenes = assets / "scenes"
    scenes.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [cli, "stage-theme"],
        input=json.dumps({"source": str(design_dir), "destination": str(assets), "palette": "blue"}),
        text=True, check=True, stdout=subprocess.DEVNULL,
    )
    for profile in profiles:
        system = profile["systems"][0] if profile["systems"] else "megadrive"
        dest = assets / "stage" / profile["id"]
        dest.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [cli, "stage-controls"],
            input=json.dumps({
                "system": system,
                "source": str(controllers),
                "design": str(design_dir),
                "destination": str(dest),
                "controls": {"profile": profile["id"]},
            }),
            text=True, check=True, stdout=subprocess.DEVNULL,
        )
        scene = dest / f"scene-{profile['id']}.rml"
        if not scene.is_file():
            print(f"FAIL {design}/{profile['id']}: exporter wrote no scene", file=sys.stderr)
            failed = True
            continue
        (scenes / f"{profile['id']}.rml").write_text(scene.read_text())
        # Where the exporter's layout puts every leader run and ring, by the
        # stop they belong to, in the order of scene-geometry. A pad with no
        # drawing has none.
        marks = []
        if profile.get("image"):
            asked = subprocess.run(
                [cli, "scene-geometry"],
                input=json.dumps({"system": system, "profile": profile["id"], "design": str(design_dir)}),
                text=True, check=True, capture_output=True,
            )
            layout = json.loads(asked.stdout)["result"]
            for placed in layout["controls"]:
                for mark in [*placed["leader"], placed["marker"]]:
                    marks.append(f"control-{placed['id']}\t{mark['x']}\t{mark['y']}")
            for group in layout["groups"]:
                if group["marker"]:
                    for mark in [*group["leader"], group["marker"]]:
                        marks.append(f"control-group-{group['name']}\t{mark['x']}\t{mark['y']}")
        (scenes / f"{profile['id']}.marks").write_text("".join(line + "\n" for line in marks))
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
        (scenes / f"{profile['id']}.lines").write_text("\n".join(lines) + "\n")
    # We measure the list in the document of one export, which has the
    # bind rows and the volume control, and swap in the scenes above.
    shell = assets / "stage" / profiles[0]["id"]
    menu = shell / "menu.rml"
    if not menu.is_file():
        print(f"FAIL {design}: exporter wrote no menu", file=sys.stderr)
        failed = True
        continue
    (assets / "menu.rml").write_bytes(menu.read_bytes())
    completed = subprocess.run(
        [binary, str(assets), "placement", str(scenes), design, str(width)],
    )
    if completed.returncode != 0:
        failed = True

if failed:
    sys.exit(1)
PY

# Test the C++ menu frame/action lifecycle against the same staged Native
# export as above. We replace only the RetroArch host commands. Config parsing,
# file writes, declarations and the RmlUi document are the production code.
# All eight declared Mega Drive callouts are active, so we can move through
# the scene by pointer or keyboard. These are fixture bindings, no runtime remap.
python3 - "$build_dir/placement-native/controls-defaults.cfg" <<'FIXTURE'
import pathlib
import sys
ids = ("up", "left", "right", "down", "y", "b", "a", "start")
pathlib.Path(sys.argv[1]).write_text(''.join(
    f'rib_label_{id} = "{id}"\ninput_player1_{id} = "a"\n' for id in ids
))
FIXTURE
python3 - "$build_dir/placement-native" "$repo_root/desktop/controls.json" <<'FIXTURE'
import json
import pathlib
import sys

assets = pathlib.Path(sys.argv[1])
menu = assets / "menu.rml"
markup = menu.read_text()
anchor = '<div id="unlock-row"'
assert anchor in markup
assert '<button class="menu-action options-back"' in markup
markup = markup.replace(
    anchor,
    '<div id="fixture-panel" class="screen-panel" '
    'style="display:none;position:absolute;left:100dp;top:100dp;width:600dp;height:300dp;">'
    '<div class="list" style="width:500dp;"><div class="list-page">'
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
menu.write_text(markup)
declarations = assets / "design.cfg"
config = declarations.read_text()
assert 'screens = "pause options controls"' in config
assert 'screen_button_options = "options"' in config
config = config.replace('screens = "pause options controls"',
                        'screens = "pause options controls fixture"', 1)
config = config.replace('screen_button_options = "options"',
                        'screen_button_options = "options fixture-back"', 1)
config += ('\nscreen_panel_fixture = "fixture-panel"'
           '\nscreen_heading_fixture = "TEST LIST"'
           '\nscreen_footer_fixture = "ESC BACK"'
           '\nscreen_button_fixture = "fixture"\n')
declarations.write_text(config)

profiles = json.loads(pathlib.Path(sys.argv[2]).read_text())["profiles"]
assert max(len(profile["controls"]) for profile in profiles) <= 48, "declared profile exceeds the preserved player limit"
profile = next(profile for profile in profiles if profile["id"] == "ps1-analog")
ids = [control["id"] for control in profile["controls"]]
assert len(ids) == 24
ids += [f"extra{index:02d}" for index in range(25)]
defaults = assets / "stage" / "ps1-analog" / "controls-defaults.cfg"
(defaults.parent / "expected-controls.txt").write_text("\n".join(ids[:48]) + "\n")
for support in ("menu.rcss", "Silkscreen-Regular.ttf"):
    (defaults.parent / support).write_bytes((assets / support).read_bytes())
groups = {control["id"]: control.get("group") for control in profile["controls"]}
defaults.write_text(
    'controls_profile = "ps1-analog"\n'
    + 'controls_variant_controls_ps1-analog = "' + ' '.join(ids) + '"\n'
    + ''.join(f'rib_label_{id} = "{id}"\ninput_player1_{id} = "a"\n' for id in ids)
    + ''.join(f'rib_group_{id} = "{group}"\n' for id, group in groups.items() if group)
)
FIXTURE
libretro_common=$repo_root/vendor/retroarch/libretro-common
harness "$build_dir/test_menu_orchestration" --define HAVE_AUDIOMIXER \
  "$script_dir/test_menu_orchestration.cpp" \
  "$script_dir/menu_host_fake.cpp" \
  "$script_dir/text_test_host.cpp" \
  "$libretro_common/file/config_file.c" "$libretro_common/file/file_path.c" \
  "$libretro_common/file/file_path_io.c" "$libretro_common/streams/file_stream.c" \
  "$libretro_common/string/stdstring.c" "$libretro_common/vfs/vfs_implementation.c" \
  "$libretro_common/encodings/encoding_utf.c" "$libretro_common/time/rtime.c" \
  "$libretro_common/compat/compat_strl.c"
# A whole Native menu as in a game with filters and achievements, staged by
# the exporter, for the cases that choose rows on generated list screens.
python3 - "$build_dir/placement-everything" "$repo_root" <<'EVERYTHING'
import json
import pathlib
import subprocess
import sys
assets = pathlib.Path(sys.argv[1])
root = pathlib.Path(sys.argv[2])
cli = subprocess.check_output(["python3", str(root / "scripts/built.py")], text=True).strip()
design = root / "integrations/designs/native"
assets.mkdir(parents=True, exist_ok=True)
subprocess.run([cli, "stage-theme"], input=json.dumps(
    {"source": str(design), "destination": str(assets), "palette": "blue"}),
    text=True, check=True, stdout=subprocess.DEVNULL)
subprocess.run([cli, "stage-controls"], input=json.dumps({
    "system": "megadrive", "source": str(root / "desktop/assets/controllers"),
    "design": str(design), "destination": str(assets), "palette": "blue",
    "includeAchievements": True, "shaders": {"bundled": ["scanlines", "phosphor"]},
}), text=True, check=True, stdout=subprocess.DEVNULL)
EVERYTHING
# A failing workflow may save a configuration before it reports the failure.
# Use the project's scratch context so every run starts with fixed inputs.
PYTHONPATH="$repo_root/scripts" python3 - "$build_dir" <<'ORCHESTRATION'
from pathlib import Path
import subprocess
import sys
from scratch import scratch
build = Path(sys.argv[1])
with scratch("rominabox-menu-orchestration-") as data:
    subprocess.run([str(build / "test_menu_orchestration"),
                    str(build / "placement-native"), data], check=True)
with scratch("rominabox-menu-capacity-") as data:
    subprocess.run([str(build / "test_menu_orchestration"), "--capacity",
                    str(build / "placement-native/stage/ps1-analog"), data], check=True)
ORCHESTRATION

if [ "$row_edge_failed" -ne 0 ]; then
  exit 1
fi
