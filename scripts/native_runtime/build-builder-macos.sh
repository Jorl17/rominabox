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
# One directory per design, so that the kit contains the designs by name and
# not one unnamed set of menu files. Each new design is a new directory here.
designs_root="$root/integrations/designs"
# The controller artwork is common to every design, because the pads are the
# same in each. At export we read it from the kit's shared directory, so we
# copy it there. A copy only in the native design's directory would leave the
# shared copy out of date, and export would then stop with
# "Could not prepare controller artwork" for a console whose drawing is
# missing from the shared directory.
shared_assets="$root/desktop/src-tauri/resources/runtime/menu-assets"
menu_source="$designs_root/native"
controller_source="$root/desktop/assets/controllers"
branding_source="$root/desktop/assets/branding"
# A design package is a directory, so we copy the whole directory. A list of
# its documents here could miss a file added to the design, and the kit would
# then contain an older copy than the source we copied it from.
[ -d "$menu_source" ] || { echo "Missing design package: $menu_source" >&2; exit 1; }
# We share the prepared kit between a worktree and the checkout it was made
# from, through a symlink, so that we do not rebuild RetroArch in each one. We
# treat the shared kit as read-only, because copying a design through the
# symlink would write into the other checkout, and every other worktree would
# then have a kit that does not match its own design.
kit_root=$root/desktop/src-tauri/resources/runtime
if [ -L "$kit_root" ]; then
  cat >&2 <<MESSAGE
This checkout shares its prepared runtime kit with another one:

  $kit_root -> $(readlink "$kit_root")

Staging into it would change that checkout's kit, and every worktree linked to
it. If this worktree needs its own kit — which it does if it is changing a
design or the fork — ask for one:

  python3 scripts/worktree.py create <name> --own-runtime

MESSAGE
  exit 1
fi
# We take the declared illustrations from the catalog, so adding a console
# does not require extending a list here.
controller_pngs=$(cargo run --quiet \
  --manifest-path "$root/desktop/crates/rominabox-catalog/Cargo.toml" \
  --bin rominabox-catalog -- assets) || {
  echo 'Could not ask the catalog which controller assets to stage' >&2
  exit 1
}
for asset in $controller_pngs CONTROLLERS.txt; do
  [ -f "$controller_source/$asset" ] || { echo "Missing controller source: $asset" >&2; exit 1; }
done
# We give Tauri this directory as the runtime resource. Refresh the authored
# menu and controller assets without freezing or rebuilding the runtime kit.
# We copy every design, because in the staleness check we go through
# integrations/designs and reject any design that is not in the kit.
for design in "$designs_root"/*; do
  [ -d "$design" ] || continue
  name=$(basename "$design")
  dest="$root/desktop/src-tauri/resources/runtime/designs/$name"
  mkdir -p "$dest"
  cp -R "$design/." "$dest/"
done
# We put the pads in the kit's shared directory and not in one design, because
# the pads are the same in every design and we read them from there at export.
mkdir -p "$shared_assets"
for asset in $controller_pngs; do
  cp -p "$controller_source/$asset" "$shared_assets/$asset"
done
cp -p "$controller_source/CONTROLLERS.txt" "$shared_assets/CONTROLLERS.txt"
# At export we copy the selected menu sound pack from the runtime kit, so the
# kit contains exactly the packs in desktop/assets/menu-sounds. A pack is one
# complete set of up/down/ok/cancel. We remove packs that are no longer in use
# from the staging folder, so nobody can select or bundle them.
sound_source="$root/desktop/assets/menu-sounds"
sound_staging="$root/desktop/src-tauri/resources/runtime/sound-packs"
[ -f "$sound_source/PROVENANCE.txt" ] || { echo 'Missing menu sounds; run node scripts/native_runtime/generate-menu-sounds.mjs' >&2; exit 1; }
mkdir -p "$sound_staging"
for staged in "$sound_staging"/*/; do
  [ -d "$staged" ] || continue
  name=$(basename "$staged")
  [ -d "$sound_source/$name" ] || rm -rf -- "$sound_staging/$name"
done
for pack in "$sound_source"/*/; do
  [ -d "$pack" ] || continue
  name=$(basename "$pack")
  mkdir -p "$sound_staging/$name"
  for cue in up down ok cancel; do
    [ -f "$pack$cue.wav" ] || { echo "Menu sound pack $name is missing $cue.wav" >&2; exit 1; }
    cp -p "$pack$cue.wav" "$sound_staging/$name/$cue.wav"
  done
done
cp -p "$sound_source/PROVENANCE.txt" "$sound_staging/PROVENANCE.txt"
mkdir -p "$root/desktop/src-tauri/resources/runtime/branding"
cp "$branding_source/logo.png" "$root/desktop/src-tauri/resources/runtime/branding/logo.png"
cp "$branding_source/PROVENANCE.txt" "$root/desktop/src-tauri/resources/runtime/branding/PROVENANCE.txt"
# In a Tauri build, resources are copied with their permissions, and a frozen
# library can be read-only. Make these generated copies writable by the owner
# so we can replace them in the next build. We do not follow symlinks here.
find "$root/desktop/src-tauri/resources" -type f ! -perm -u=w -exec chmod u+w '{}' +
for profile in debug release; do
  staging="$root/desktop/src-tauri/target/$profile"
  if [ -d "$staging" ]; then
    find "$staging" -type f -name '*.dylib' ! -perm -u=w -exec chmod u+w '{}' +
  fi
done
cd "$root/desktop/src-tauri"
cargo build --release --features custom-protocol --bin rominabox-cli
mkdir -p resources/bin resources/skills/rominabox
cp target/release/rominabox-cli resources/bin/rominabox-cli
cp "$root/skills/rominabox/SKILL.md" resources/skills/rominabox/SKILL.md
cd "$root/desktop"
npm run tauri build -- --bundles app
# productName in desktop/src-tauri/tauri.conf.json is ROM-in-a-Box.
app="$root/desktop/src-tauri/target/release/bundle/macos/ROM-in-a-Box.app"
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
