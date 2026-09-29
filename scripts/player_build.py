"""The player build for a launched test, and the player inside it.

A test that launches the player gets the exact build from
ROMINABOX_TEST_BUILD. That build must come from the current fork, or the
test proves something about older code.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Iterable

sys.path.insert(0, str(Path(__file__).resolve().parent))
import core_source  # noqa: E402
import native_build  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
HOW = "build one with python3 scripts/build_player.py <absolute dir> and name it in ROMINABOX_TEST_BUILD"


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


def newest_script_build(builds: Iterable[Path], revision: str) -> Path | None:
    """The newest of `builds` that a launched test can run: built from the fork
    commit `revision`, with the menu script driver, and containing its player.
    A build from another commit proves nothing about the code of this one."""
    usable = [build for build in builds
              if runs_scripts(build) and info(build).get("retroarchCommit") == revision
              and player_in(build).is_file()]
    return max(usable, key=lambda build: player_in(build).stat().st_mtime, default=None)


def selected_build() -> Path:
    """The build in ROMINABOX_TEST_BUILD, refused unless it is from the current fork."""
    selected = os.environ.get("ROMINABOX_TEST_BUILD")
    if not selected:
        raise SystemExit(f"no player build selected; {HOW}")
    return current_build(Path(selected))


def current_build(named: Path) -> Path:
    """`named`, refused unless it was built from the current fork and contains a player."""
    build = named.resolve()
    revision = current_revision()
    if info(build).get("retroarchCommit") != revision:
        raise SystemExit(f"{build} was not built from the current fork commit {revision}")
    if not player_in(build).is_file():
        raise SystemExit(f"the selected build contains no player: {player_in(build)}")
    return build
