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
  make -j"$jobs" \
    RMLUI_SOURCE_DIR=../vendor/RmlUi \
    RMLUI_BUILD_DIR=../build-rmlui
  strip retroarch
)

printf '{"retroarchCommit":"%s","rmluiCommit":"ba95ffe8bfb6370efb2cdcca927eaad4710c5413"}\n' "$retroarch_commit" > "$destination/build-info.json"
echo "Built $destination/retroarch/retroarch from $retroarch_commit"
