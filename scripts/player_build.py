"""The player build for a launched test, and the player inside it.

A launched test runs the test player of this checkout, which we build from
the current fork commit, or the build in ROMINABOX_TEST_BUILD, which we refuse
unless it was built from that same commit. Either way the test proves
something about the code in the tree, not about a build somebody made by hand.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import core_source  # noqa: E402
import native_build  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
# The test player of this checkout: one folder, built again in place, so after
# the first build we compile only what changed in the fork.
TEST_BUILD = ROOT / "work/test-player"
# A test player has the menu script driver, which we use in every launched
# test to drive the menu, and sends achievements only to a loopback host.
TEST_SWITCHES = {"ROMINABOX_MENU_SCRIPT_BUILD": "1", "ROMINABOX_ACHIEVEMENTS_TEST_BUILD": "1"}


def info(build: Path) -> dict:
    return json.loads((build / "build-info.json").read_text(encoding="utf-8"))


def player_in(build: Path) -> Path:
    """The player binary inside a build directory, with the name for its target.

    A build whose record has no target was built for the host.
    """
    target = info(build).get("target", core_source.host_target())
    return build / "retroarch" / native_build.binary_name(target)


def current_revision() -> str:
    """The fork commit from which we build the player in this checkout."""
    return subprocess.check_output(
        ["git", "-C", str(ROOT / "vendor/retroarch"), "rev-parse", "HEAD"], text=True
    ).strip()


def runs_scripts(build: Path) -> bool:
    """Whether the player in the build has the menu script driver, which we
    use in every launched test to drive the menu."""
    record = build / "build-info.json"
    return record.is_file() and info(build).get("capabilities", {}).get("menuScript") is True


def selected_build() -> Path:
    """The build in ROMINABOX_TEST_BUILD, refused unless it is from the current
    fork, or else the test player of this checkout, built now. We then put the
    test player in ROMINABOX_TEST_BUILD, so the programs started from here run
    it and build nothing."""
    selected = os.environ.get("ROMINABOX_TEST_BUILD")
    if selected:
        return current_build(Path(selected))
    built = subprocess.run([sys.executable, str(ROOT / "scripts/build_player.py"), str(TEST_BUILD)],
                           env={**os.environ, **TEST_SWITCHES}, capture_output=True, text=True, errors="replace")
    if built.returncode != 0:
        raise SystemExit(f"could not build the test player in {TEST_BUILD}:\n{(built.stdout + built.stderr)[-4000:]}")
    os.environ["ROMINABOX_TEST_BUILD"] = str(TEST_BUILD)
    return current_build(TEST_BUILD)


def current_build(named: Path) -> Path:
    """`named`, refused unless it was built from the current fork and contains a player."""
    build = named.resolve()
    revision = current_revision()
    if info(build).get("retroarchCommit") != revision:
        raise SystemExit(f"{build} was not built from the current fork commit {revision}")
    if not player_in(build).is_file():
        raise SystemExit(f"the selected build contains no player: {player_in(build)}")
    return build
