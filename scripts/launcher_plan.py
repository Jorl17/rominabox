"""The game's launcher as a plan tool, for the checks in which we read what we
write in a launch without starting a game.

With ROMINABOX_PLAN_ONLY we prepare the game's data folder and config in the
launcher and exit before a core is loaded. We build it for this machine in
this checkout's folder for that build (native_build.tree_build), and copy it
to where an exported game has its program, beside the folder with its own
files.
"""

from __future__ import annotations

import os
import re
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
from core_source import host_target  # noqa: E402

CONTRACT = Path(__file__).resolve().parent.parent / "desktop/src-tauri/launcher/launch_contract.inc"


def declared(kind: str, name: str) -> str:
    """The path declared in launch_contract.inc as `kind(name, "path")`, as we
    read it in the launcher."""
    found = re.search(rf'^{kind}\({name}, "([^"]+)"\)', CONTRACT.read_text(encoding="utf-8"), re.MULTILINE)
    if not found:
        raise SystemExit(f"launch_contract.inc declares no {kind} {name}")
    return found.group(1)


def windows_part(name: str) -> str:
    """Where part `name` of a Windows game is beside its program."""
    return declared("RIB_WINDOWS_PART", name)


# The plan tools built in this process. We build the macOS one in the first
# case of a run and copy it in every later case.
_built: set[Path] = set()


def compile_macos(directory: Path) -> tuple[Path, Path]:
    binary = directory / "Plan.app" / "Contents" / "MacOS" / "plan"
    binary.parent.mkdir(parents=True)
    # The system libraries the launch library links, built from the same sources.
    library = native_build.launch_library(native_build.kit_target(host_target()))
    if library is None:
        raise SystemExit(f"the player recipe declares no launch library for {host_target()}")
    with native_build.tree_build(f"plan-{host_target()}") as folder:
        tool = folder / "plan"
        if folder not in _built:
            native_build.run(["cc", "-DROMINABOX_PLAN_MAIN", "-O2", "-o", str(tool),
                              *native_build.launcher_includes("macos"),
                              *map(str, native_build.launcher_sources("macos")), *map(str, native_build.LIBRARIES),
                              *library["libraries"]],
                             folder, dict(os.environ))
            _built.add(folder)
        shutil.copy2(tool, binary)
    return binary, binary.parents[1] / "Resources"


def compile_windows(directory: Path) -> tuple[Path, Path]:
    """The game's launcher, as in the kit, which is the plan tool."""
    game = directory / "Plan"
    binary = native_build.tree_launcher(native_build.kit_target(host_target()), game / "Plan.exe")
    return binary, game / "Resources"


# The plan tool, and the folder of its game's own files, on each platform.
PLANS = {"macos": compile_macos, "windows": compile_windows}


def compile_plan(directory: Path) -> tuple[Path, Path]:
    """The plan tool built in `directory`, and the folder of its game's own files."""
    platform = host_target().split("-", 1)[0]
    if platform not in PLANS:
        raise SystemExit(f"no launcher plan tool is declared for {platform}")
    return PLANS[platform](directory)
