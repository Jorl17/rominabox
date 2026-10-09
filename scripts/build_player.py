"""Build the player (the RetroArch fork with the RmlUi menu) into a build folder.

    uv run python scripts/build_player.py /absolute/build/folder         # this machine's target
    uv run python scripts/build_player.py --target windows-x86_64 DIR
    uv run python scripts/build_player.py --target macos-universal DIR   # the macOS kit's player

To build a folder again, pass it again. We bring the fork's committed source
there to its current commit and compile only what changed. A folder contains
the build of one target, and one build at a time.

For a universal target we build each slice as its own target, in a folder
named after the slice inside DIR, and join their players into DIR/retroarch.
In the same build we make the game's launcher, where the recipe has one, and
the builder's menu preview renderer (rml-preview). A player for the tests that launch it has the menu's script driver
(ROMINABOX_MENU_SCRIPT_BUILD=1), and a player we ship does not. With either
test switch (the other is ROMINABOX_ACHIEVEMENTS_TEST_BUILD=1) the build is
test-only, and we never freeze it into a kit. What we build is in
scripts/native_runtime/player-recipe.json, and how in scripts/native_build.py.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import core_source  # noqa: E402
import native_build  # noqa: E402
from folder_lock import Lock  # noqa: E402

# The target of a build folder, which we write when its first build starts.
TARGET_RECORD = "build-target"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("destination", type=Path, help="an absolute directory: a new one, or one to build again")
    parser.add_argument("--target", default=core_source.host_target(), help="default: this machine's")
    arguments = parser.parse_args()
    target = native_build.require_build_target(arguments.target)
    destination: Path = arguments.destination
    if not destination.is_absolute():
        raise SystemExit("DESTINATION must be an absolute path")

    commit = native_build.fork_commit()
    switches = {"achievements_test": os.environ.get("ROMINABOX_ACHIEVEMENTS_TEST_BUILD", "0"),
                "menu_script": os.environ.get("ROMINABOX_MENU_SCRIPT_BUILD", "0")}
    destination.mkdir(parents=True, exist_ok=True)
    with Lock(destination / ".building", busy=f"another build is using {destination}", wait=0):
        claim(destination, target)
        parts = native_build.slices(target)
        if parts == [target]:
            build_slice(destination, target, commit, switches)
        else:
            for part in parts:
                build_slice(destination / part, part, commit, switches)
            join(destination, target, parts)
            write_info(destination, target, commit, switches, parts)
    print(f"Built {destination / 'retroarch' / native_build.binary_name(target)} for {target} from {commit}")
    return 0


def claim(destination: Path, target: str) -> None:
    """Make `destination` the build folder for `target`. A new or empty folder
    becomes one, and a build folder stays one. We build its FreeType and
    RmlUi for its target, so we refuse a folder of another target or of
    anything else. We write build-info.json last, so until this build
    finishes, nothing in the folder marks any build as complete."""
    record = destination / TARGET_RECORD
    if record.is_file():
        held = record.read_text(encoding="utf-8").strip()
        if held != target:
            raise SystemExit(f"{destination} is a {held} build folder, not {target}")
    elif any(entry.name != ".building" for entry in destination.iterdir()):
        raise SystemExit(f"{destination} is not a build folder: name a new folder or one a build made")
    else:
        record.write_text(target + "\n", encoding="utf-8", newline="\n")
    (destination / "build-info.json").unlink(missing_ok=True)


def build_slice(destination: Path, target: str, commit: str, switches: dict[str, str]) -> None:
    """Build the player for one target in `destination`, with the launcher and
    the preview renderer that the recipe lists beside it."""
    jobs = os.cpu_count() or 1
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "build-info.json").unlink(missing_ok=True)
    native_build.checkout_fork(commit, destination, target)
    rmlui_build = native_build.build_rmlui(destination, target, jobs)
    retroarch = destination / "retroarch"
    sources = native_build.copy_sources(destination, target)
    binary = retroarch / native_build.binary_name(target)
    # We link the player unstripped with make, and read its symbols there in
    # the capability check.
    linked = native_build.linked(binary)
    rmlui = native_build.recipe()["rmlui"]
    settings = {"RIB_ACHIEVEMENTS_TEST": switches["achievements_test"],
                "RIB_MENU_SCRIPT": switches["menu_script"],
                "RMLUI_SOURCE_DIR": f"../{rmlui['source']}", "RMLUI_BUILD_DIR": f"../{rmlui_build.name}",
                "RIB_SOURCES_DIR": native_build.make_path(sources, target),
                "TARGET": linked.stem,
                **native_build.make_variables(target)}
    native_build.write_if_changed(retroarch / "Makefile.local",
                                  native_build.makefile_local(target, settings).encode("utf-8"))

    native_build.write_if_changed(retroarch / "configure.mk", native_build.configure_makefile(target).encode("utf-8"))

    if native_build.builds_sdl2(target):
        native_build.build_sdl2(destination, target, jobs)
    environment = {**native_build.build_environment(target), **native_build.freetype_environment(destination),
                   **native_build.sdl2_environment(destination, target)}
    native_build.run(["make", "-f", "configure.mk"], retroarch, environment)
    native_build.run(["make", f"-j{jobs}"], retroarch, environment)

    for name, function in native_build.recipe()["capabilities"].items():
        if name != "comment" and not native_build.has_symbol(linked, target, function, environment):
            raise SystemExit(f"The built {target} player has no {name} ({function})")
    native_build.run(["strip", "-o", str(binary), str(linked)], retroarch, environment)
    launcher = native_build.build_launcher(destination, target, environment)
    preview = native_build.build_preview(destination, target, environment, rmlui_build)
    for built in [binary, *([launcher] if launcher else []), *([preview] if preview else [])]:
        foreign = native_build.foreign_imports(built, target, environment)
        if foreign:
            raise SystemExit(f"{built.name} links libraries {target} does not have: {', '.join(foreign)}")
    write_info(destination, target, commit, switches)
    others = [str(path) for path in (launcher, preview) if path]
    print(f"Built {binary}" + (f", and {', '.join(others)}" if others else ""), flush=True)


def join(destination: Path, target: str, parts: list[str]) -> None:
    """Join the players of the slices into one file, which must contain a slice
    for each processor, each linked only to the system's libraries. We checked
    the capabilities in every slice before we stripped it, because after
    stripping no symbol is left to read."""
    name = native_build.binary_name(target)
    joined = destination / "retroarch" / name
    joined.parent.mkdir(exist_ok=True)
    environment = native_build.build_environment(parts[0])
    native_build.run(["lipo", "-create", "-output", str(joined),
                      *(str(destination / part / "retroarch" / name) for part in parts)], destination, environment)
    wanted = {native_build.architecture_of(part) for part in parts}
    held = native_build.architectures_in(joined)
    if held != wanted:
        raise SystemExit(f"{joined} holds {sorted(a.value for a in held)}, not {sorted(a.value for a in wanted)}")
    for part in parts:
        foreign = native_build.foreign_imports(joined, part, environment)
        if foreign:
            raise SystemExit(f"The {part} slice of the joined player links {', '.join(foreign)}")
    # We sign every slice ad hoc, as macOS requires for arm64 code, because
    # linking leaves some slices unsigned.
    native_build.run(["codesign", "--force", "--sign", "-", str(joined)], destination, environment)


def write_info(destination: Path, target: str, commit: str, switches: dict[str, str],
               parts: list[str] | None = None) -> None:
    """Describe the build: its sources, its target (and the slices of a
    universal one), its capabilities, and whether it is only for tests."""
    info = {
        "retroarchCommit": commit, "rmluiCommit": native_build.recipe()["rmlui"]["commit"], "target": target,
        **({"slices": parts} if parts else {}),
        "capabilities": {"achievements": True, "menuScript": switches["menu_script"] == "1"},
        "testOnly": switches["achievements_test"] == "1" or switches["menu_script"] == "1",
    }
    (destination / "build-info.json").write_text(json.dumps(info) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    sys.exit(main())
