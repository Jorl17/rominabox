#!/bin/sh
# Headless RmlUi interaction checks, with no RetroArch and no window.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bridge_dir=$repo_root/vendor/retroarch/menu/drivers
# The one declaration of the library path. The binary stays in this checkout,
# so one checkout never runs another's tests.
rmlui_lib=$(python3 "$repo_root/scripts/rmlui_paths.py" library)
rmlui_includes=$(python3 "$repo_root/scripts/rmlui_paths.py" includes)
design=$repo_root/integrations/designs/native
build_dir=$repo_root/work/bridge-interaction
# The stylesheet that we write in an export, not the file of the design. A
# design has design(surface) where a colour goes, and we fill it in at export,
# so the file of the design alone gives RmlUi tokens that it cannot parse.
assets=$build_dir/assets
out=$build_dir/test_rmlui_interaction

if [ ! -f "$bridge_dir/rmlui_bridge.cpp" ]; then
  echo "missing experimental RmlUi bridge at $bridge_dir" >&2
  exit 1
fi
if [ ! -f "$rmlui_lib" ]; then
  echo "missing $rmlui_lib (python3 scripts/prepare_rmlui.py)" >&2
  exit 1
fi

mkdir -p "$build_dir"
freetype_cflags=$(pkg-config --cflags freetype2)
freetype_libs=$(pkg-config --libs freetype2)

c++ -std=c++17 -DRIB_RMLUI_HEADLESS \
  $rmlui_includes -I "$bridge_dir" -I "$repo_root/vendor/retroarch/libretro-common/include" $freetype_cflags \
  -o "$out" \
  "$script_dir/test_rmlui_interaction.cpp" \
  "$bridge_dir/rmlui_bridge.cpp" \
  "$bridge_dir/rmlui/declarations.cpp" \
  "$bridge_dir/rmlui/binds_popup.cpp" \
  "$bridge_dir/rmlui/document.cpp" \
  "$script_dir/test_menu_declarations.cpp" \
  "$rmlui_lib" \
  $freetype_libs

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

# A generated list screen, in the same form that we write into <!--SCREENS-->
# at export. We add it after staging the stylesheet, because staging copies the
# menu.rml of the design over this one. Without it the bridge has no list, and
# the checks of the rows and controls on a list screen cannot fail.
python3 - "$assets/menu.rml" <<'FIXTURE'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
panel = (
    '<div id="fixture-panel" class="screen-panel" style="display:none;">'
    '<div class="list"><div id="fixture-page-1" class="list-page">'
    '<button id="fixture-one" class="list-row line"><div id="fixture-one-title" class="list-row-title">ONE</div>'
    '<div id="fixture-one-state" class="list-row-state"></div></button>'
    '<button id="fixture-rest" class="list-row line"><div id="fixture-rest-title" class="list-row-title">REST</div>'
    '<div id="fixture-rest-state" class="list-row-state"></div></button>'
    '<button id="fixture-two" class="list-row "><div id="fixture-two-title" class="list-row-title">TWO</div>'
    '<div id="fixture-two-detail" class="list-row-detail">detail</div>'
    '<div id="fixture-two-state" class="list-row-state"></div></button>'
    '<button id="fixture-pic" class="list-row "><div id="fixture-pic-title" class="list-row-title">PIC</div>'
    '<div id="fixture-pic-detail" class="list-row-detail">detail</div>'
    '<div id="fixture-pic-state" class="list-row-state"></div></button>'
    '</div></div>'
    '<div class="list-actions">'
    '<button class="menu-action list-toggle" id="fixture-mode">'
    '<span class="list-toggle-label">MODE</span>'
    '<span id="fixture-mode-state" class="list-toggle-state">OFF</span></button>'
    '<button class="menu-action list-back" id="fixture-back">BACK</button>'
    '</div></div>'
    '<div id="control-binds" class="list" style="display:none;">'
    '<div class="list-page">'
    '<button id="bind-one" class="list-row line"><div id="bind-one-title" class="list-row-title">ONE</div></button>'
    '<button id="bind-rest" class="list-row line"><div id="bind-rest-title" class="list-row-title">REST</div></button>'
    '</div></div>'
)
document = p.read_text()
if "<!--SCREENS-->" not in document:
    raise SystemExit("menu.rml has no <!--SCREENS--> slot for the list fixture")
p.write_text(document.replace("<!--SCREENS-->", panel))
FIXTURE

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
  python3 - "$edge_assets/menu.rml" <<'FIXTURE'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
panel = (
    '<div id="fixture-panel" class="screen-panel" style="display:none;">'
    '<div class="list"><div id="fixture-page-1" class="list-page">'
    '<button id="fixture-one" class="list-row line"><div id="fixture-one-title" class="list-row-title">ONE</div>'
    '<div id="fixture-one-state" class="list-row-state"></div></button>'
    '<button id="fixture-rest" class="list-row line"><div id="fixture-rest-title" class="list-row-title">REST</div>'
    '<div id="fixture-rest-state" class="list-row-state"></div></button>'
    '<button id="fixture-two" class="list-row "><div id="fixture-two-title" class="list-row-title">TWO</div>'
    '<div id="fixture-two-detail" class="list-row-detail">detail</div>'
    '<div id="fixture-two-state" class="list-row-state"></div></button>'
    '<button id="fixture-pic" class="list-row "><div id="fixture-pic-title" class="list-row-title">PIC</div>'
    '<div id="fixture-pic-detail" class="list-row-detail">detail</div>'
    '<div id="fixture-pic-state" class="list-row-state"></div></button>'
    '</div></div></div>'
    '<div id="control-binds" class="list" style="display:none;">'
    '<div class="list-page">'
    '<button id="bind-one" class="list-row line"><div id="bind-one-title" class="list-row-title">ONE</div></button>'
    '<button id="bind-rest" class="list-row line"><div id="bind-rest-title" class="list-row-title">REST</div></button>'
    '</div></div>'
)
document = p.read_text()
if "<!--SCREENS-->" not in document:
    raise SystemExit("menu.rml has no <!--SCREENS--> slot for the list fixture")
p.write_text(document.replace("<!--SCREENS-->", panel))
FIXTURE
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
assert "<!--SCREENS-->" in markup
assert '<button class="menu-action options-back"' in markup
markup = markup.replace(
    "<!--SCREENS-->",
    '<div id="fixture-panel" class="screen-panel" '
    'style="display:none;position:absolute;left:100dp;top:100dp;width:600dp;height:300dp;">'
    '<div class="list" style="width:500dp;"><div class="list-page">'
    '<button id="fixture-one" class="list-row" style="width:400dp;height:42dp;">ONE</button>'
    '<button id="fixture-two" class="list-row" style="width:400dp;height:42dp;">TWO</button>'
    '</div></div><button id="fixture-back" class="menu-action list-back">BACK</button>'
    '</div>',
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
orchestration_objects=""
for source in \
  file/config_file.c file/file_path.c file/file_path_io.c \
  streams/file_stream.c string/stdstring.c vfs/vfs_implementation.c \
  encodings/encoding_utf.c time/rtime.c compat/compat_strl.c; do
  object=$build_dir/$(basename "$source" .c)-orchestration.o
  cc -I "$libretro_common/include" -c "$libretro_common/$source" -o "$object"
  orchestration_objects="$orchestration_objects $object"
done
c++ -std=c++17 -DRIB_RMLUI_HEADLESS \
  $rmlui_includes -I "$bridge_dir" -I "$libretro_common/include" $freetype_cflags \
  -o "$build_dir/test_menu_orchestration" \
  "$script_dir/test_menu_orchestration.cpp" \
  "$bridge_dir/rmlui/menu.cpp" \
  "$bridge_dir/rmlui/controls.cpp" \
  "$bridge_dir/rmlui/overlays.cpp" \
  "$bridge_dir/rmlui/script.cpp" \
  "$bridge_dir/rmlui/shaders.cpp" \
  "$bridge_dir/rmlui/discs.cpp" \
  "$bridge_dir/rmlui/settings.cpp" \
  "$bridge_dir/rmlui_bridge.cpp" \
  "$bridge_dir/rmlui/declarations.cpp" \
  "$bridge_dir/rmlui/binds_popup.cpp" \
  "$bridge_dir/rmlui/document.cpp" \
  "$bridge_dir/rmlui/files.cpp" \
  $orchestration_objects "$rmlui_lib" $freetype_libs
mkdir -p "$build_dir/orchestration-data"
"$build_dir/test_menu_orchestration" \
  "$build_dir/placement-native" "$build_dir/orchestration-data"
"$build_dir/test_menu_orchestration" --capacity \
  "$build_dir/placement-native/stage/ps1-analog" "$build_dir/orchestration-data"

if [ "$row_edge_failed" -ne 0 ]; then
  exit 1
fi
