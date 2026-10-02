#!/bin/sh
# RmlUi input and presentation, with stand-ins for external services. No window.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"
. "$root/scripts/python.sh"
eval "$(py scripts/worktree.py env)"
build="$root/work/account-input"
harness() { py scripts/native_runtime/menu_harness.py build "$@"; }
host="scripts/native_runtime/account_test_host.cpp"
service="scripts/native_runtime/achievements_fake.cpp"
harness "$build/probe" scripts/native_runtime/test_account_input.cpp "$host" "$service" scripts/native_runtime/text_test_host.cpp
harness "$build/unlock-popup" scripts/native_runtime/test_unlock_popup.cpp "$host" "$service" scripts/native_runtime/text_test_host.cpp
probes="$build/probe:$build/unlock-popup"
if [ "$(uname -s)" = Darwin ]; then
  harness "$build/composition" --define HAVE_COCOA --framework AppKit \
    scripts/native_runtime/test_text_composition.mm "$host" "$service" scripts/native_runtime/text_test_host.cpp
  probes="$probes:$build/composition"
fi
ROMINABOX_INPUT_PROBE="$probes" cargo test --manifest-path desktop/crates/rominabox-engine/Cargo.toml --test design_composition live_achievements -- --nocapture
