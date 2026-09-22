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
    '<button id="fixture-one" class="list-row "><div class="list-row-title">ONE</div>'
    '<div id="fixture-one-state" class="list-row-state"></div></button>'
    '<button id="fixture-two" class="list-row "><div class="list-row-title">TWO</div>'
    '<div id="fixture-two-state" class="list-row-state"></div></button>'
    '</div></div>'
    '<div class="list-actions">'
    '<button class="menu-action list-toggle" id="fixture-mode">'
    '<span class="list-toggle-label">MODE</span>'
    '<span id="fixture-mode-state" class="list-toggle-state">OFF</span></button>'
    '<button class="menu-action list-back" id="fixture-back">BACK</button>'
    '</div></div>'
)
document = p.read_text()
if "<!--SCREENS-->" not in document:
    raise SystemExit("menu.rml has no <!--SCREENS--> slot for the list fixture")
p.write_text(document.replace("<!--SCREENS-->", panel))
FIXTURE

"$out" "$assets" "$build_dir/thumbnail-test.png"
