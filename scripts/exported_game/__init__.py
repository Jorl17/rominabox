"""An exported game on this computer, as we use it in a harness that launches
it: its launcher, its launch plan, its storage and log, and its processes.

What differs by platform is in macos.py and windows.py, which have the same
functions: the launcher, the folder of the game's own files, the per-user
folder that the plan's $user_data stands for, whether the game runs in a
sandbox, the folder that must contain its per-game storage, how we prepare
that storage before writing into it from a harness, which of its processes
are still running, and the folder we use in a harness while the game is
open, with what we remove afterwards. Here we choose the module for the
platform we export games for on this machine.
"""

from __future__ import annotations

import shutil
import sys
from collections.abc import Callable
from contextlib import AbstractContextManager
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from core_source import host_target  # noqa: E402

from . import launch, macos, windows  # noqa: E402
from .launch import QUIET_ENV, SCRIPT_ENV  # noqa: E402,F401

PLATFORM = host_target().split("-", 1)[0]
# The module for each platform we export games for.
PLATFORMS = {"macos": macos, "windows": windows}


def platform():
    """The module for the platform we export games for on this machine."""
    if PLATFORM not in PLATFORMS:
        raise SystemExit(f"no exported game is declared for {PLATFORM}")
    return PLATFORMS[PLATFORM]


def quiet_env() -> str:
    return QUIET_ENV


def launcher_of(app: Path) -> Path:
    return platform().launcher(app)


def sandboxed(app: Path) -> bool:
    return platform().sandboxed(app)


def resources_of(app: Path) -> Path:
    """The folder of an exported game's own files."""
    return platform().resources(app)


def plan_text(app: Path) -> str:
    return launch.read_plan(resources_of(app))


def data_dir_of(app: Path) -> Path | None:
    """The per-game storage, as we compute it in the launcher."""
    return launch.data_dir(plan_text(app), platform().user_data(app))


def log_of(app: Path) -> Path | None:
    """The file to which we send the player's output in the launcher."""
    data = data_dir_of(app)
    if data is None:
        return None
    return data / "logs/launch.log"


def storage_home(app: Path) -> Path | None:
    """The folder a game's own storage must lie inside, or None when the
    game's storage is not contained."""
    return platform().storage_home(app)


def running_from(app: Path) -> str:
    """The game's processes still running, one per line, or empty when none is."""
    return platform().running(app)


def opened(exported: Path) -> AbstractContextManager[tuple[Path, Callable[[], None]]]:
    """The folder of the game we exported to `exported`, as we use it in a
    harness while the block runs, and `keep`. When the block ends, however it
    ends, we remove everything from the game on this computer, unless we
    called `keep` for a player of the game that may still be running."""
    return platform().opened(exported)


# The games whose storage we have prepared in this run, once each.
_prepared: set[Path] = set()


def prepare_storage(app: Path) -> None:
    """The game's storage as it is after the first launch, before we write into
    it from a harness."""
    if app.resolve() in _prepared:
        return
    platform().prepare_storage(app)
    _prepared.add(app.resolve())


def prepared_storage(app: Path) -> Path | None:
    """The game's own storage, prepared before we write into it from a
    harness, or None for a game without one."""
    data = data_dir_of(app)
    if data is not None:
        prepare_storage(app)
    return data


def shot_inside(app: Path, target: Path) -> Path:
    """The path where we tell the game to write the picture for `target`.

    A sandboxed game has write access only inside its own storage, not in
    this repository. So the picture goes into the game's storage, and we
    move it out with `carry_shot`. With a path in the tree, every shot would
    end in "failed to open file for writing", because of the sandbox.
    """
    data = prepared_storage(app)
    if data is None or not sandboxed(app):
        return target
    inside = data / "shots" / target.name
    inside.parent.mkdir(parents=True, exist_ok=True)
    inside.unlink(missing_ok=True)
    return inside


def carry_shot(inside: Path, target: Path) -> None:
    """Move the picture from the game's own storage to `target`."""
    if inside != target and inside.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(inside), str(target))
