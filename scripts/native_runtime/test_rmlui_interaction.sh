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
# Cargo writes to the default target only when nothing redirects it, and a
# worktree always redirects it.
cli=${CARGO_TARGET_DIR:-$repo_root/desktop/src-tauri/target}/release/rominabox-cli
if [ -x "$cli" ]; then
  printf '{"source":"%s","destination":"%s","palette":"blue"}' "$design" "$assets" \
    | "$cli" stage-theme >/dev/null || {
      echo "could not stage the design's stylesheet" >&2; exit 1; }
else
  echo "build the CLI first: cargo build --release --bin rominabox-cli" >&2
  exit 1
fi

"$out" "$assets" "$build_dir/thumbnail-test.png"
