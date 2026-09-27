"""Make a target's runtime kit, from which builders package games, from a player
build, and install the builder's menu preview renderer that we build beside it.

    python3 scripts/build_kit.py BUILD [KIT]    # default KIT: desktop/src-tauri/resources/runtime

BUILD is a directory we made with scripts/build_player.py for a player we
ship, not a test build. Its build-info.json contains the target. Every kit
has the same assets (scripts/kit_assets.py). The target's part of
scripts/native_runtime/player-recipe.json lists the rest: the player, the
launcher, the licence texts of what they are made from, and the player's
controller profile folders, which we stage from the pinned autoconfig archive.
We freeze the player with each library it links that is not part of the
system (on macOS, Homebrew's FreeType and mbedTLS), and with no other.
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
import kit_assets  # noqa: E402
import native_build  # noqa: E402
import prepare_runtime  # noqa: E402
import toolchain  # noqa: E402
from built import cli as built_cli  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent


def source_of(reference: str, build: Path) -> Path:
    """Return a file the recipe lists, where build: is in the player build and
    toolchain: in the toolchain's installation."""
    place, _, relative = reference.partition(":")
    if place == "build":
        return build / relative
    if place == "toolchain":
        prefix = toolchain.installation()
        if prefix is None:
            raise SystemExit(f"the kit recipe names a toolchain file, but this toolchain is the system's: {reference}")
        return prefix / relative
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
    target = native_build.require_target(info["target"])
    declared = native_build.recipe()["kit"].get(target)
    if declared is None:
        raise SystemExit(f"the player recipe declares no kit for {target}")

    kit_assets.stage(kit)
    for placed in declared["files"].values():
        (kit / placed["at"]).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source_of(placed["from"], build), kit / placed["at"])
    # We write every native licence again below, so the kit contains only the
    # licences of libraries the player links. We remove the existing files
    # first, because on a case-insensitive file system we would otherwise
    # write a new name into an old file.
    native_licences = kit / "licenses" / "native"
    if native_licences.is_dir():
        for stale in native_licences.iterdir():
            if stale.is_file():
                stale.unlink()
    for relative, reference in declared["licences"].items():
        source = source_of(reference, build)
        (kit / "licenses" / relative).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, kit / "licenses" / relative)

    bundled = bundle_libraries(kit, declared, build) if "libraries" in declared else []
    native, notice = native_dependencies(target, declared, bundled)
    (kit / "licenses" / "NATIVE-DEPENDENCIES.txt").write_text(
        f"Native dependency provenance for the {target} runtime kit.\n"
        "Private ROM-in-a-Box solution-discovery artifact, not a publication decision.\n\n"
        "The player is built by scripts/build_player.py from the pinned ROM-in-a-Box\n"
        f"RetroArch fork at {info['retroarchCommit']}, with the toolchain\n"
        "scripts/toolchain.py declares for its target. Linked into it:\n\n"
        f"- RmlUi at {info['rmluiCommit']} (licence: ../RmlUi-MIT.txt)\n"
        f"{notice}\n"
        "RetroArch's own licence is ../RetroArch.txt; the libraries its source\n"
        "carries under deps/ are part of that source.\n",
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
    }, indent=2) + "\n", encoding="utf-8", newline="\n")

    player = kit / declared["files"]["player"]["at"]
    platform, _, architecture = target.partition("-")
    manifest = {
        "schema_version": 1,
        "platform": platform,
        "architecture": architecture,
        "components": [
            {
                "name": "RetroArch",
                "license": "GPL-3.0",
                "license_file": "licenses/RetroArch.txt",
                "revision": info["retroarchCommit"],
                "capabilities": info["capabilities"],
                "origin": "Built from the pinned ROM-in-a-Box RetroArch submodule; private development build.",
                "source_url": f"https://github.com/Jorl17/rominabox-retroarch/tree/{info['retroarchCommit']}",
                "binary_sha256": sha256(player),
                "integration_provenance": "provenance/native-rmlui/source.json",
            },
            {
                "name": "RmlUi",
                "license": "MIT",
                "license_file": "licenses/RmlUi-MIT.txt",
                "revision": info["rmluiCommit"],
                "origin": "Static library linked into the player.",
            },
            *native,
        ],
        "branding": {"logo": "branding/logo.png"},
    }
    (kit / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
    # After the manifest, because we add the profiles' component record to it.
    prepare_runtime.stage_joypad_autoconfig(kit, native_build.joypad_profile_drivers(platform))
    print(f"Made the {target} runtime kit in {kit} from {build}")

    install_preview(build, target)
    return 0


def bundle_libraries(kit: Path, declared: dict, build: Path) -> list[dict]:
    """Freeze the player with each library it links that is not part of the
    system, copying each from its installation into the declared folder with
    the licence files of that installation. Remove anything else in that
    folder, so the kit has only what this player loads."""
    libraries = declared["libraries"]
    if libraries["from"] != "homebrew":
        raise SystemExit(f"unknown library source {libraries['from']!r}")
    player = kit / declared["files"]["player"]["at"]
    folder = kit / libraries["folder"]
    frozen = Path(tempfile.mkdtemp(prefix="frozen-", dir=build)) / player.name
    subprocess.run([str(built_cli(build=True)), "freeze-macos-executable"], check=True, text=True,
                   input=json.dumps({"source": str(source_of(declared["files"]["player"]["from"], build)),
                                     "destination": str(frozen)}))
    names = sorted(library.name for library in (frozen.parent / "Frameworks").iterdir())
    folder.mkdir(exist_ok=True)
    for stale in folder.iterdir():
        if stale.name not in names:
            stale.unlink()
    shutil.copy2(frozen, player)
    for name in names:
        # Freezing leaves them read-only, and we could not replace them next time.
        shutil.copyfile(frozen.parent / "Frameworks" / name, folder / name)

    # The frozen program has the paths of libraries beside it. In the kit
    # they are in the folder, relative to the player.
    beside = "@executable_path/Frameworks/"
    placed = f"@executable_path/{os.path.relpath(folder, player.parent)}/"
    for binary in [*(folder / name for name in names), player]:
        listed = subprocess.run(["otool", "-L", str(binary)], check=True, capture_output=True, text=True).stdout
        for line in listed.splitlines()[1:]:
            dependency = line.strip().split(" (")[0]
            if dependency.startswith(beside):
                subprocess.run(["install_name_tool", "-change", dependency,
                                placed + dependency[len(beside):], str(binary)], check=True, capture_output=True)
        subprocess.run(["codesign", "--force", "--sign", "-", str(binary)], check=True, capture_output=True)

    (kit / "runtime-dependencies.json").write_text(json.dumps({
        "formatVersion": 1,
        "files": [{"name": name, "sha256": sha256(folder / name)} for name in names],
    }, indent=2) + "\n", encoding="utf-8", newline="\n")

    bundled = []
    for name in names:
        keg = (Path(libraries["prefix"]) / "lib" / name).resolve().parents[1]
        formula = keg.parent.name
        licences = [found for found in sorted(keg.iterdir())
                    if found.is_file() and found.name.upper().startswith(("LICENSE", "COPYING"))]
        if not licences:
            raise SystemExit(f"{keg} carries no licence file for {name}")
        (kit / "licenses" / "native").mkdir(parents=True, exist_ok=True)
        for licence in licences:
            shutil.copyfile(licence, kit / "licenses" / "native" / f"{formula}-{licence.name}")
        bundled.append({"library": name, "formula": formula, "version": keg.name,
                        "licences": [f"{formula}-{licence.name}" for licence in licences]})
    return bundled


def native_dependencies(target: str, declared: dict, bundled: list[dict]) -> tuple[list[dict], str]:
    """The records in the manifest of the native libraries other than RmlUi,
    and their lines in NATIVE-DEPENDENCIES.txt: FreeType, which we link from
    its pinned release, or each library we copy beside the player."""
    freetype = native_build.recipe()["freetype"][target]
    if not bundled:
        return [{
            "name": "FreeType",
            "license": "FTL or GPL-2.0",
            "license_file": "licenses/native/FreeType-LICENSE.txt",
            "origin": f"Static library linked into the player, from {freetype['url']}.",
        }], (f"- FreeType from {freetype['url']}, sha256 {freetype['sha256']}\n"
             "  (licence: native/FreeType-LICENSE.txt, native/FreeType-FTL.txt)\n"
             f"- {declared['runtime']}\n")
    formulas = {}
    for library in bundled:
        formulas.setdefault((library["formula"], library["version"]), []).append(library)
    folder = declared["libraries"]["folder"]
    records = [{
        "name": formula,
        "revision": version,
        "license_file": f"licenses/native/{libraries[0]['licences'][0]}",
        "origin": f"{', '.join(library['library'] for library in libraries)}, copied into {folder} "
                  f"from the {declared['libraries']['from']} installation of {formula} {version}.",
    } for (formula, version), libraries in sorted(formulas.items())]
    lines = "".join(
        f"- {formula} {version}: {', '.join(library['library'] for library in libraries)}, copied into "
        f"{folder} (licence: {', '.join('native/' + name for name in libraries[0]['licences'])})\n"
        for (formula, version), libraries in sorted(formulas.items()))
    return records, lines


def install_preview(build: Path, target: str) -> None:
    """The builder's menu preview renderer, which we build beside the player,
    in the folder we load it from in the builder and the picture tests."""
    declared = native_build.recipe()["preview"].get(target)
    if not declared:
        return
    built = build / "preview" / declared["output"]
    if not built.is_file():
        raise SystemExit(f"{build} has no menu preview renderer at {built}")
    installed = native_build.preview_resource(target)
    installed.parent.mkdir(parents=True, exist_ok=True)
    if declared["install"] == "copy":
        shutil.copy2(built, installed)
    elif declared["install"] == "freeze":
        # We freeze into a new folder of the build and then copy it over the
        # installed one, because freezing fails when the destination exists.
        frozen = Path(tempfile.mkdtemp(prefix="frozen-", dir=build / "preview")) / declared["output"]
        subprocess.run([str(built_cli(build=True)), "freeze-macos-executable"], check=True,
                       input=json.dumps({"source": str(built), "destination": str(frozen)}), text=True)
        shutil.copy2(frozen, installed)
        (installed.parent / "Frameworks").mkdir(exist_ok=True)
        for library in (frozen.parent / "Frameworks").iterdir():
            # Freezing leaves libraries read-only. We could then not replace them
            # in the next install or in the copy of the resources in a Tauri build.
            shutil.copyfile(library, installed.parent / "Frameworks" / library.name)
    else:
        raise SystemExit(f"unknown preview install {declared['install']!r} for {target}")
    print(f"Installed the menu preview renderer as {installed}")


if __name__ == "__main__":
    sys.exit(main())
