#!/bin/sh
# Headless RmlUi interaction checks, with no RetroArch and no window.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bridge_dir=$repo_root/vendor/retroarch/menu/drivers
# Uses a prebuilt RmlUi from the work tree.
rmlui_lib=$repo_root/work/experiments/rml-retroarch/build-rmlui/librmlui.a
rmlui_inc=$repo_root/work/experiments/rml-retroarch/vendor/RmlUi/Include
assets=$repo_root/integrations/designs/native
build_dir=$repo_root/work/experiments/rml-retroarch/build-interaction-test
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

# The template has a placeholder where the volume control goes, which we fill
# at export. The test must use that document, or it clicks a comment.
staged=$build_dir/menu-assets
rm -rf "$staged"
cp -R "$assets" "$staged"
eval "$(python3 "$repo_root/scripts/worktree.py" env)"
cli=${CARGO_TARGET_DIR:-$repo_root/desktop/src-tauri/target}/release/rominabox-cli
if [ ! -x "$cli" ]; then
  cargo build --release --manifest-path "$repo_root/desktop/src-tauri/Cargo.toml" --bin rominabox-cli
fi
python3 - "$cli" "$staged" <<'PY'
import json, pathlib, subprocess, sys
cli, staged = sys.argv[1:]
result = subprocess.run(
    [cli, "volume-markup"],
    input=json.dumps({"design": staged}),
    text=True, capture_output=True, check=True)
markup = json.loads(result.stdout)["result"]["markup"]
document = pathlib.Path(staged) / "menu.rml"
text = document.read_text()
slot = "<!--VOLUME-->"
if slot not in text:
    raise SystemExit("menu.rml has no volume slot to fill")
document.write_text(text.replace(slot, markup, 1))
PY

"$out" "$staged" "$build_dir/thumbnail-test.png"
