#!/bin/sh
# RmlUi input and presentation, with stand-ins for external services. No window.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"
eval "$(python3 scripts/worktree.py env)"
bridge="$root/vendor/retroarch/menu/drivers"
lib=$(python3 scripts/rmlui_paths.py library)
includes=$(python3 scripts/rmlui_paths.py includes)
build="$root/work/account-input"
mkdir -p "$build"
common="text_entry screens document achievements lists live_lists binds_popup overlays"
sources=""
for module in $common; do sources="$sources $bridge/rmlui/$module.cpp"; done
cxxflags="-std=c++17 -Werror=return-type -DRIB_RMLUI_HEADLESS $includes -I$bridge -I$root/vendor/retroarch/libretro-common/include"
ft_cflags=$(pkg-config --cflags freetype2)
ft_libs=$(pkg-config --libs freetype2)
host="$root/scripts/native_runtime/account_test_host.cpp"
c++ $cxxflags $ft_cflags scripts/native_runtime/test_account_input.cpp "$host" scripts/native_runtime/text_test_host.cpp $sources "$lib" $ft_libs -o "$build/probe"
probes="$build/probe"
if [ "$(uname -s)" = Darwin ]; then
  c++ $cxxflags $ft_cflags -DHAVE_COCOA scripts/native_runtime/test_text_composition.mm "$host" scripts/native_runtime/text_test_host.cpp $sources "$lib" $ft_libs -framework AppKit -o "$build/composition"
  probes="$probes:$build/composition"
fi
ROMINABOX_INPUT_PROBE="$probes" cargo test --manifest-path desktop/src-tauri/Cargo.toml --test design_composition live_achievements -- --nocapture
