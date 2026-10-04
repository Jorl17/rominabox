"""Make a target's runtime kit, from which builders package games, from a player
build, and install the builder's menu preview renderer that we build beside it.

    uv run python scripts/build_kit.py BUILD [KIT]    # default KIT: desktop/src-tauri/resources/runtime

BUILD is a directory we made with scripts/build_player.py for a player we
ship, not a test build. Its build-info.json contains the target, which for
macOS is macos-universal. Every kit has the same assets (scripts/kit_assets.py).
The target's part of scripts/native_runtime/player-recipe.json lists the rest:
the player, the launcher, and the player's controller profile folders, which
we stage from the pinned autoconfig archive. The licence texts of what the
player is made from are the entries in licenses/ (scripts/licences.py) of
every library compiled in the build. When a library has no entry yet, we
name it in a warning and make the kit all the same. The kit's
licenses/index.json lists these entries and those of the data in every game
and of the fonts. We read it in the exporter to write each game's index. We
link only the system's libraries into the player, so the library folder in a
macOS kit is empty and its inventory lists no file.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import built  # noqa: E402
import kit_assets  # noqa: E402
import licences  # noqa: E402
import native_build  # noqa: E402
import prepare_runtime  # noqa: E402
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
# The pictures of the builder's Menu step, in the builder's resources.
PICTURES = ROOT / "desktop/src-tauri/resources/menu-previews"
# The environment variable with the path of a folder with a software OpenGL
# for Windows.
OPENGL_LIBRARIES = "ROMINABOX_OPENGL_LIBRARIES"


def source_of(reference: str, build: Path) -> Path | None:
    """Return a file the recipe lists, where build: is in the player build, or
    None for made:, a file we make for the kit in this script."""
    place, _, relative = reference.partition(":")
    if place == "build":
        return build / relative
    if place == "made":
        return None
    raise SystemExit(f"the kit recipe names an unknown place: {reference}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    if len(sys.argv) not in (2, 3):
        raise SystemExit("usage: build_kit.py BUILD [KIT]")
    build = Path(sys.argv[1]).resolve()
    kit = Path(sys.argv[2]).resolve() if len(sys.argv) == 3 else kit_assets.KIT
    info = json.loads((build / "build-info.json").read_text(encoding="utf-8"))
    if info.get("testOnly"):
        raise SystemExit("A test-only build must never be frozen into the distributable kit")
    if info.get("capabilities", {}).get("achievements") is not True:
        raise SystemExit("The build has no verified achievements capability")
    target = native_build.require_build_target(info["target"])
    declared = native_build.recipe()["kit"].get(target)
    if declared is None:
        raise SystemExit(f"the player recipe declares no kit for {target}")

    platform, _, architecture = target.partition("-")
    native, missing = licences.player_components(build, platform)
    missing += licences.toolchain_differences(native, toolchain.installation())
    kit_assets.stage(kit)
    for placed in declared["files"].values():
        source = source_of(placed["from"], build)
        if source is not None:
            (kit / placed["at"]).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, kit / placed["at"])
    # A macOS kit's launcher is a library we load into the player. We make it
    # here from the launcher sources and attach it to the player we just
    # copied. For a Windows kit we build it with the player and only copy it.
    library = native_build.launch_library(target)
    if library:
        native_build.install_tree_launcher(kit, target)
    # The entries for everything a game made from the kit can include, and
    # their index: the player's libraries, the data in every game, and the
    # fonts, of which a game keeps those of its design. We write every entry
    # again, so no entry is left for a library the player no longer links. We
    # remove the old entries first, because on a case-insensitive file system
    # we would otherwise write a new name into an old file.
    shipped = [*native,
               *(component for component in licences.sources.declared_components(
                   "data", licences.sources.declared()["data"]) if component.declared.get("inGames")),
               *licences.sources.fonts()]
    for group in ("native", "data", "fonts"):
        folder = kit / "licenses" / group
        folder.mkdir(parents=True, exist_ok=True)
        for stale in folder.iterdir():
            if stale.is_file():
                stale.unlink()
    for component in shipped:
        shutil.copy2(licences.OUT / licences.entry_path(component), kit / "licenses" / licences.entry_path(component))
    licences.write_index(kit / "licenses", shipped)

    if "libraryInventory" in declared:
        empty_library_inventory(kit, declared["libraryInventory"])
    (kit / "licenses" / "NATIVE-DEPENDENCIES.txt").write_text(
        f"Native dependency provenance for the {target} runtime kit.\n\n"
        "The player is built by scripts/build_player.py from the pinned ROM-in-a-Box\n"
        f"RetroArch fork at {info['retroarchCommit']}, with RmlUi at {info['rmluiCommit']}\n"
        "and the toolchain scripts/toolchain.py declares for its target. It is made\n"
        "from these, each with its licence in native/:\n\n"
        + "".join(f"- {component.title} {component.version} (native/{component.name}.txt)\n"
                  for component in native),
        encoding="utf-8", newline="\n")
    # Every export contains this folder, so it has only the record of this build.
    provenance = kit / "provenance" / "native-rmlui"
    provenance.mkdir(parents=True, exist_ok=True)
    for stale in provenance.iterdir():
        if stale.is_file():
            stale.unlink()
    (provenance / "source.json").write_text(json.dumps({
        "retroarchCommit": info["retroarchCommit"],
        "rmluiCommit": info["rmluiCommit"],
        "target": target,
        "recipe": "scripts/native_runtime/player-recipe.json",
        **({"launchLibrarySources": native_build.launch_library_sources(target)} if library else {}),
    }, indent=2) + "\n", encoding="utf-8", newline="\n")

    player = kit / declared["files"]["player"]["at"]
    records = {component.name: {
        "name": component.title,
        "license": component.licence,
        "license_file": f"licenses/{licences.entry_path(component).as_posix()}",
        "version": component.version,
        "source": component.source,
        "origin": f"Used by {component.used_by}.",
    } for component in native}
    # The player itself: RetroArch, built from the fork's commit.
    records["retroarch"].update({
        "revision": info["retroarchCommit"],
        "capabilities": info["capabilities"],
        "origin": "Built from the pinned ROM-in-a-Box RetroArch submodule.",
        "source_url": f"https://github.com/Jorl17/rominabox-retroarch/tree/{info['retroarchCommit']}",
        "binary_sha256": sha256(player),
        "integration_provenance": "provenance/native-rmlui/source.json",
    })
    manifest = {
        "schema_version": 1,
        "platform": platform,
        "architecture": architecture,
        "components": list(records.values()),
        "branding": {"logo": "branding/logo.png"},
    }
    (kit / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
    # After the manifest, because we add the profiles' component record to it.
    prepare_runtime.stage_joypad_autoconfig(kit, native_build.joypad_profile_drivers(platform))
    print(f"Made the {target} runtime kit in {kit} from {build}")

    renderer = install_preview(build, target)
    if renderer:
        render_menu_previews(kit, renderer, target)
    if missing:
        print(licences.warning(missing), file=sys.stderr)
    return 0


def empty_library_inventory(kit: Path, declared: dict) -> None:
    """Write the library folder and the inventory we read in the exporter for a
    player that uses no library: an empty folder and an inventory that lists
    no file. Remove any library we copied there for an earlier kit."""
    folder = kit / declared["folder"]
    folder.mkdir(exist_ok=True)
    for stale in folder.iterdir():
        if stale.is_file() or stale.is_symlink():
            stale.unlink()
    (kit / declared["file"]).write_text(json.dumps({"formatVersion": 1, "files": []}, indent=2) + "\n",
                                        encoding="utf-8", newline="\n")


def install_preview(build: Path, target: str) -> Path | None:
    """The builder's menu preview renderer, which we build beside the player
    for each slice that has one in the recipe, in the folder we load it from
    in the builder and the picture tests. For a universal target we join the
    renderers of the slices into one file, as we join the players. We link
    only the system's libraries into it, so we remove any library an earlier
    renderer left beside it."""
    parts = [part for part in native_build.slices(target) if native_build.recipe()["preview"].get(part)]
    if not parts:
        return None
    built = [(build if part == target else build / part) / "preview" / native_build.recipe()["preview"][part]["output"]
             for part in parts]
    for path in built:
        if not path.is_file():
            raise SystemExit(f"{path.parent.parent} has no menu preview renderer at {path}")
    installed = native_build.preview_resource(parts[0])
    installed.parent.mkdir(parents=True, exist_ok=True)
    if len(built) == 1:
        shutil.copy2(built[0], installed)
    else:
        native_build.run(["lipo", "-create", "-output", str(installed), *map(str, built)], build,
                         native_build.build_environment(parts[0]))
    carried = installed.parent / "Frameworks"
    if carried.is_dir():
        for library in carried.iterdir():
            if library.is_file() or library.is_symlink():
                library.unlink()
        carried.rmdir()
    print(f"Installed the menu preview renderer as {installed}")
    return installed


def render_menu_previews(kit: Path, renderer: Path, target: str) -> None:
    """The pictures of the builder's Menu step, of every design in every
    palette, from `kit` with `renderer`, in the builder's resources. Each file
    name is a digest of what the picture is drawn from (menu::render_preview).
    In the builder we show the file instead of rendering a picture with the
    same name. We remove the pictures of an earlier kit.

    On a Windows computer without a graphics card, such as a CI runner, the
    environment variable OPENGL_LIBRARIES contains the path of a folder with a
    software OpenGL (Mesa's opengl32.dll and libgallium_wgl.dll). We draw with a copy of the renderer
    beside those libraries, because Windows loads opengl32.dll from the
    program's own folder first, and the libraries stay out of the builder."""
    declared = json.loads((ROOT / "desktop/designs.json").read_text(encoding="utf-8"))
    PICTURES.mkdir(parents=True, exist_ok=True)
    names = set()
    cli = built.cli()
    with tempfile.TemporaryDirectory() as scratch:
        libraries = os.environ.get(OPENGL_LIBRARIES)
        if libraries and native_build.is_windows(target):
            drawing = Path(scratch) / "renderer"
            drawing.mkdir()
            for library in Path(libraries).glob("*.dll"):
                shutil.copy2(library, drawing)
            renderer = Path(shutil.copy2(renderer, drawing))
        for design in declared["designs"]:
            for palette in declared["palettes"]:
                request = {"outputDir": str(Path(scratch) / f"{design['id']}-{palette['id']}"),
                           "theme": design["id"], "palette": palette["id"],
                           "design": str(kit / "designs" / design["id"]), "assets": str(kit / "menu-assets"),
                           "renderer": str(renderer)}
                run = subprocess.run([str(cli), "preview"], input=json.dumps(request),
                                     capture_output=True, text=True, encoding="utf-8")
                events = [json.loads(line) for line in run.stdout.splitlines() if line.startswith("{")]
                result = next((event["result"] for event in events if event.get("type") == "result"), None)
                if run.returncode != 0 or result is None:
                    raise SystemExit(f"could not render {design['id']} in {palette['id']}: {run.stdout}{run.stderr}")
                shutil.copy2(result["imagePath"], PICTURES / result["name"])
                names.add(result["name"])
    for stale in PICTURES.glob("*.png"):
        if stale.name not in names:
            stale.unlink()
    print(f"Rendered {len(names)} menu pictures into {PICTURES}")


if __name__ == "__main__":
    sys.exit(main())
