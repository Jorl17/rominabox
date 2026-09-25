#!/bin/sh
# Compile the managed boundary with the actual rcheevos client and evaluator
# and a synthetic in-process service. We use no RetroArch window or account.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
retroarch="$repo_root/vendor/retroarch"
output="$repo_root/work/test-output/achievements-runtime-client-test"
mkdir -p "$repo_root/work/test-output"

clang -std=gnu99 -O0 -g -Wno-deprecated-declarations -DRC_NO_THREADS \
  -ffunction-sections -fdata-sections -Wl,-dead_strip \
  -I"$retroarch" -I"$retroarch/deps" \
  -I"$retroarch/libretro-common/include" \
  -I"$retroarch/deps/rcheevos/include" \
  -I"$repo_root/desktop/src-tauri/accounts" \
  -I"$repo_root/desktop/src-tauri/launcher" \
  "$repo_root/scripts/achievements_runtime_client_test.c" \
  "$retroarch/cheevos/rominabox.c" \
  "$retroarch/cheevos/rominabox_catalog.c" \
  "$retroarch/cheevos/rominabox_storage.c" \
  "$repo_root/desktop/src-tauri/accounts/accounts.c" \
  "$repo_root/desktop/src-tauri/accounts/sealed.c" \
  "$repo_root/desktop/src-tauri/launcher/portable_fs.c" \
  "$retroarch/deps/rcheevos/src/rc_client.c" \
  "$retroarch/deps/rcheevos/src/rc_compat.c" \
  "$retroarch/deps/rcheevos/src/rc_util.c" \
  "$retroarch"/deps/rcheevos/src/rcheevos/*.c \
  "$retroarch/deps/rcheevos/src/rapi/rc_api_common.c" \
  "$retroarch/deps/rcheevos/src/rapi/rc_api_runtime.c" \
  "$retroarch/deps/rcheevos/src/rapi/rc_api_user.c" \
  "$retroarch/libretro-common/utils/md5.c" \
  -o "$output"

"$output"
