#!/bin/sh
# Build only, without starting the builder or an emulator.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
case "${1:-}" in
  -h|--help) echo 'Usage: scripts/native_runtime/build-builder-macos.sh'; echo 'Requires npm, cargo, macOS build tools, and prepared resources/runtime and resources/preview.'; exit 0 ;;
  '') ;;
  *) echo 'Unknown argument; use --help.' >&2; exit 1 ;;
esac
[ "$(uname -s)" = Darwin ] || { echo 'This build requires macOS.' >&2; exit 1; }
[ -f "$root/desktop/src-tauri/resources/runtime/bin/retroarch" ] || { echo 'Prepare the native runtime kit first.' >&2; exit 1; }
[ -f "$root/desktop/src-tauri/resources/preview/rml-preview" ] || { echo 'Prepare the offscreen preview helper first.' >&2; exit 1; }
# Resolve from Cargo's working directory, so that a relative CARGO_TARGET_DIR
# is the same folder in the build and in the copy and signing steps below.
cargo_target_dir=$(cd "$root/desktop/src-tauri" && PYTHONPATH="$root/scripts" python3 -c 'from built import target_dir; print(target_dir().resolve())')
# We copy the kit's own assets (designs, parts, controller pictures, sound
# packs, branding) in the same way on every platform.
python3 "$root/scripts/kit_assets.py" "$root/desktop/src-tauri/resources/runtime"
# In a Tauri build, resources are copied with their permissions, and a frozen
# library can be read-only. Make these generated copies writable by the owner
# so we can replace them in the next build. We do not follow symlinks here.
find "$root/desktop/src-tauri/resources" -type f ! -perm -u=w -exec chmod u+w '{}' +
for profile in debug release; do
  staging="$cargo_target_dir/$profile"
  if [ -d "$staging" ]; then
    find "$staging" -type f -name '*.dylib' ! -perm -u=w -exec chmod u+w '{}' +
  fi
done
cd "$root/desktop/src-tauri"
cargo build --release --features custom-protocol --bin rominabox-cli
mkdir -p resources/bin resources/skills/rominabox
cp "$cargo_target_dir/release/rominabox-cli" resources/bin/rominabox-cli
cp "$root/skills/rominabox/SKILL.md" resources/skills/rominabox/SKILL.md
cd "$root/desktop"
npm run tauri build -- --bundles app
# productName in desktop/src-tauri/tauri.conf.json is ROM-in-a-Box.
app="$cargo_target_dir/release/bundle/macos/ROM-in-a-Box.app"
[ -d "$app" ] || { echo "Missing builder bundle: $app" >&2; exit 1; }
# Ad-hoc signatures for developers, made after the resource copy. Sign each
# Mach-O file inside first, including dylibs without the execute bit. We do
# not follow symlinks here. Do not use --deep signing as a substitute.
find "$app" -type f -exec /bin/sh -c '
  set -eu
  for path
  do
    case "$(LC_ALL=C /usr/bin/file -b -- "$path")" in
      Mach-O*)
        /usr/bin/codesign --force --sign - -- "$path" || {
          echo "Failed to ad-hoc sign: $path" >&2
          exit 1
        }
        ;;
    esac
  done
' _ {} +
/usr/bin/codesign --force --sign - -- "$app" || { echo "Failed to ad-hoc sign the builder bundle." >&2; exit 1; }
/usr/bin/codesign --verify --deep --strict -- "$app" || { echo "Failed to verify the builder bundle signature." >&2; exit 1; }
