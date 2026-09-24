#!/bin/sh
# Compiles only the HTTP connection boundary; it never opens a socket.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
retroarch="$repo_root/vendor/retroarch"
output="$repo_root/work/test-output/achievements-runtime-redirect-test"
mkdir -p "$repo_root/work/test-output"

clang -std=gnu99 -O0 -g -DRIB_ACHIEVEMENTS_TEST \
  -ffunction-sections -fdata-sections -Wl,-dead_strip \
  -I"$retroarch/libretro-common/include" \
  "$repo_root/scripts/achievements_runtime_redirect_test.c" \
  "$retroarch/libretro-common/net/net_http.c" \
  -o "$output"

"$output"
echo "test-build redirect containment passed"
