"""Build the player (the RetroArch fork with the RmlUi menu) into a new directory.

    uv run python scripts/build_player.py /absolute/new/build            # the host's target
    uv run python scripts/build_player.py --target windows-x86_64 DIR

A player for the tests that launch it has the menu's script driver
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("destination", type=Path, help="a new, absolute directory")
    parser.add_argument("--target", default=core_source.host_target(), help="default: this machine's")
    arguments = parser.parse_args()
    target = native_build.require_target(arguments.target)
    destination: Path = arguments.destination
    if not destination.is_absolute():
        raise SystemExit("DESTINATION must be an absolute path")
    if destination.exists():
        raise SystemExit(f"refusing to overwrite existing destination: {destination}")

    commit = native_build.fork_commit()
    achievements_test = os.environ.get("ROMINABOX_ACHIEVEMENTS_TEST_BUILD", "0")
    menu_script = os.environ.get("ROMINABOX_MENU_SCRIPT_BUILD", "0")
    jobs = os.cpu_count() or 1

    destination.mkdir(parents=True)
    native_build.archive_fork(commit, destination, target)
    rmlui_build = native_build.build_rmlui(destination, target, jobs)
    retroarch = destination / "retroarch"
    (retroarch / "Makefile.local").write_text(native_build.makefile_local(target), encoding="utf-8", newline="\n")
    accounts = native_build.copy_accounts(destination)

    environment = {**native_build.build_environment(target),
                   **native_build.freetype_environment(destination, target)}
    native_build.run([*native_build.shell(target), "./configure", *native_build.configure_flags(target)],
                     retroarch, environment)
    rmlui = native_build.recipe()["rmlui"]
    native_build.run(["make", f"-j{jobs}",
                      f"RIB_ACHIEVEMENTS_TEST={achievements_test}", f"RIB_MENU_SCRIPT={menu_script}",
                      f"RMLUI_SOURCE_DIR=../{rmlui['source']}", f"RMLUI_BUILD_DIR=../{rmlui_build.name}",
                      f"RIB_ACCOUNTS_DIR={native_build.make_path(accounts, target)}"],
                     retroarch, environment)

    binary = retroarch / native_build.binary_name(target)
    capability = native_build.recipe()["capability"]
    if not native_build.has_symbol(binary, target, capability, environment):
        raise SystemExit("The built player is missing the required achievements client integration")
    (destination / "build-info.json").write_text(json.dumps({
        "retroarchCommit": commit, "rmluiCommit": rmlui["commit"], "target": target,
        "capabilities": {"achievements": True, "menuScript": menu_script == "1"},
        "testOnly": achievements_test == "1" or menu_script == "1",
    }) + "\n", encoding="utf-8", newline="\n")
    native_build.run(["strip", str(binary)], retroarch, environment)
    if native_build.is_windows(target):
        foreign = native_build.foreign_imports(binary, environment)
        if foreign:
            raise SystemExit(f"{binary.name} needs DLLs Windows does not have: {', '.join(foreign)}")

    print(f"Built {binary} from {commit}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
