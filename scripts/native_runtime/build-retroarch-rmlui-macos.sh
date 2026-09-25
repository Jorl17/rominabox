#!/bin/sh
# Rebuild the macOS native RmlUi RetroArch binary in a new directory.
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 DESTINATION" >&2
  exit 64
fi

destination=$1
case "$destination" in
  /*) ;;
  *) echo "DESTINATION must be an absolute path" >&2; exit 64 ;;
esac
if [ -e "$destination" ]; then
  echo "refusing to overwrite existing destination: $destination" >&2
  exit 1
fi

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
vendor_retroarch=$repo_root/vendor/retroarch

if [ ! -e "$vendor_retroarch/.git" ]; then
  echo "missing pinned RetroArch fork at $vendor_retroarch" >&2
  exit 1
fi

if [ -n "$(git -C "$vendor_retroarch" status --porcelain)" ]; then
  echo "commit native source changes before building" >&2
  exit 1
fi
retroarch_commit=$(git -C "$vendor_retroarch" rev-parse HEAD)
jobs=$(sysctl -n hw.ncpu)

mkdir "$destination"
git -C "$vendor_retroarch" archive --format=tar --prefix=retroarch/ \
  "$retroarch_commit" | tar -x -C "$destination"

git clone https://github.com/mikke89/RmlUi.git "$destination/vendor/RmlUi"
git -C "$destination/vendor/RmlUi" checkout ba95ffe8bfb6370efb2cdcca927eaad4710c5413

cp "$script_dir/Makefile.local" "$destination/retroarch/Makefile.local"

# ROM-in-a-Box's own code the player is built with: the shared accounts
# store for QUICK SIGN IN and the launcher's portable file layer under it.
# RetroArch's build compiles these sources itself (RIB_ACCOUNTS_DIR).
accounts_build=$destination/rominabox-accounts
mkdir -p "$accounts_build/accounts" "$accounts_build/launcher"
cp "$repo_root"/desktop/src-tauri/accounts/accounts.[ch] \
  "$repo_root"/desktop/src-tauri/accounts/sealed.[ch] "$accounts_build/accounts/"
cp "$repo_root"/desktop/src-tauri/launcher/portable_fs.[ch] "$accounts_build/launcher/"

cmake -S "$destination/vendor/RmlUi" -B "$destination/build-rmlui" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DRMLUI_SAMPLES=OFF \
  -DRMLUI_LOTTIE_PLUGIN=OFF -DRMLUI_SVG_PLUGIN=OFF
cmake --build "$destination/build-rmlui" --parallel "$jobs"

(
  cd "$destination/retroarch"
  ./configure \
    --enable-cocoa \
    --enable-opengl \
    --disable-opengl_core \
    --enable-glsl \
    --disable-slang \
    --disable-builtinglslang \
    --disable-metal \
    --disable-vulkan \
    --enable-menu \
    --enable-rgui \
    --disable-xmb \
    --disable-ozone \
    --disable-materialui \
    --disable-gfx_widgets \
    --disable-freetype \
    --enable-coreaudio \
    --enable-hid \
    --disable-mfi \
    --disable-libusb \
    --disable-qt \
    --disable-sdl \
    --disable-sdl2 \
    --disable-test_drivers \
    --disable-ffmpeg \
    --disable-cg \
    --disable-langextra
  # A player the launched tests drive carries the menu's script driver
  # (ROMINABOX_MENU_SCRIPT_BUILD=1); a player that ships does not. Either
  # test switch marks the build test-only, which is never frozen into a kit.
  make -j"$jobs" RIB_ACHIEVEMENTS_TEST="${ROMINABOX_ACHIEVEMENTS_TEST_BUILD:-0}" \
    RIB_MENU_SCRIPT="${ROMINABOX_MENU_SCRIPT_BUILD:-0}" \
    RMLUI_SOURCE_DIR=../vendor/RmlUi \
    RMLUI_BUILD_DIR=../build-rmlui \
    RIB_ACCOUNTS_DIR="$accounts_build"
  python3 - "$destination" "$retroarch_commit" "${ROMINABOX_ACHIEVEMENTS_TEST_BUILD:-0}" \
    "${ROMINABOX_MENU_SCRIPT_BUILD:-0}" <<'CAPABILITY'
import json, pathlib, subprocess, sys
build = pathlib.Path(sys.argv[1])
symbols = subprocess.check_output(["nm", "-g", "retroarch"], text=True)
supports = any(line.endswith(" _rcheevos_rib_prepare_client") for line in symbols.splitlines())
if not supports:
    raise SystemExit("The built player is missing the required achievements client integration")
menu_script = sys.argv[4] == "1"
(build / "build-info.json").write_text(json.dumps({
    "retroarchCommit": sys.argv[2], "rmluiCommit": "ba95ffe8bfb6370efb2cdcca927eaad4710c5413",
    "capabilities": {"achievements": supports, "menuScript": menu_script},
    "testOnly": sys.argv[3] == "1" or menu_script
}) + "\n")
CAPABILITY
  strip retroarch
)

echo "Built $destination/retroarch/retroarch from $retroarch_commit"
