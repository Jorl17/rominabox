"""Make a target's runtime kit, from which builders package games, from a player build.

    python3 scripts/build_kit.py BUILD [KIT]    # default KIT: desktop/src-tauri/resources/runtime

BUILD is a directory we made with scripts/build_player.py for a player we
ship, not a test build. Its build-info.json contains the target. Every kit
has the same assets (scripts/kit_assets.py). The target's part of
scripts/native_runtime/player-recipe.json lists the rest: the player, the
launcher and the licence texts of what they are made from. Until we declare
the macOS part in the recipe, we freeze a macOS kit with
scripts/native_runtime/freeze-runtime-kit.mjs.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import kit_assets  # noqa: E402
import native_build  # noqa: E402
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent


def source_of(reference: str, build: Path) -> Path:
    """Return a file the recipe lists, where build: is in the player build and
    toolchain: in the toolchain's installation."""
    place, _, relative = reference.partition(":")
    if place == "build":
        return build / relative
    if place == "toolchain":
        return toolchain.msys2_root() / "ucrt64" / relative
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
        raise SystemExit(f"no kit is declared for {target}; a macOS kit is frozen by freeze-runtime-kit.mjs")

    kit_assets.stage(kit)
    for relative, reference in declared["files"].items():
        source = source_of(reference, build)
        (kit / relative).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, kit / relative)
    for relative, reference in declared["licences"].items():
        source = source_of(reference, build)
        (kit / "licenses" / relative).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, kit / "licenses" / relative)

    recipe = native_build.recipe()
    freetype = recipe["freetype"][target]
    (kit / "licenses" / "NATIVE-DEPENDENCIES.txt").write_text(
        f"Native dependency provenance for the {target} runtime kit.\n"
        "Private ROM-in-a-Box solution-discovery artifact, not a publication decision.\n\n"
        "The player is one statically linked program built by scripts/build_player.py\n"
        f"from the pinned ROM-in-a-Box RetroArch fork at {info['retroarchCommit']}, with\n"
        "the toolchain scripts/toolchain.py declares for its target. Linked into it:\n\n"
        f"- RmlUi at {info['rmluiCommit']} (licence: ../RmlUi-MIT.txt)\n"
        f"- FreeType from {freetype['url']}, sha256 {freetype['sha256']}\n"
        "  (licence: native/FreeType-LICENSE.txt, native/FreeType-FTL.txt)\n"
        "- the MinGW-w64 runtime: its C runtime, winpthreads and the GCC runtime\n"
        "  (native/MinGW-w64-runtime.txt, native/winpthreads-COPYING.txt,\n"
        "  native/GCC-runtime-exception.txt)\n\n"
        "RetroArch's own licence is ../RetroArch.txt; the libraries its source\n"
        "carries under deps/ are part of that source.\n",
        encoding="utf-8", newline="\n")

    provenance = kit / "provenance" / "native-rmlui"
    provenance.mkdir(parents=True, exist_ok=True)
    (provenance / "source.json").write_text(json.dumps({
        "retroarchCommit": info["retroarchCommit"],
        "rmluiCommit": info["rmluiCommit"],
        "target": target,
        "recipe": "scripts/native_runtime/player-recipe.json",
    }, indent=2) + "\n", encoding="utf-8", newline="\n")

    player = kit / "bin" / native_build.binary_name(target)
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
            {
                "name": "FreeType",
                "license": "FTL or GPL-2.0",
                "license_file": "licenses/native/FreeType-LICENSE.txt",
                "origin": f"Static library linked into the player, from {freetype['url']}.",
            },
        ],
        "branding": {"logo": "branding/logo.png"},
    }
    (kit / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(f"Made the {target} runtime kit in {kit} from {build}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
