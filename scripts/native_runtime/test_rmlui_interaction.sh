#!/bin/sh
# Headless RmlUi interaction checks, with no RetroArch and no window.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bridge_dir=$repo_root/vendor/retroarch/menu/drivers
# Uses a prebuilt RmlUi from the work tree.
rmlui_lib=$repo_root/work/experiments/rml-retroarch/build-rmlui/librmlui.a
rmlui_inc=$repo_root/work/experiments/rml-retroarch/vendor/RmlUi/Include
design=$repo_root/integrations/designs/native
build_dir=$repo_root/work/experiments/rml-retroarch/build-interaction-test
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
  echo "missing prebuilt librmlui.a at $rmlui_lib" >&2
  exit 1
fi

focus_status=0
node "$script_dir/test_control_focus.mjs" || focus_status=$?
node "$script_dir/test_list_focus.mjs" || focus_status=$?

mkdir -p "$build_dir"
freetype_cflags=$(pkg-config --cflags freetype2)
freetype_libs=$(pkg-config --libs freetype2)

c++ -std=c++17 -DRIB_RMLUI_HEADLESS \
  -I "$rmlui_inc" -I "$bridge_dir" $freetype_cflags \
  -o "$out" \
  "$script_dir/test_rmlui_interaction.cpp" \
  "$bridge_dir/rmlui_bridge.cpp" \
  "$rmlui_lib" \
  $freetype_libs

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
  rm -rf "$edge_assets"
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

if [ "$focus_status" -ne 0 ]; then
  exit "$focus_status"
fi
if [ "$row_edge_failed" -ne 0 ]; then
  exit 1
fi
