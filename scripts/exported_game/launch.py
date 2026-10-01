"""What a harness and an exported game's launcher share: the switches we pass
to a launch, the names declared in rominabox_launch.h, and the launch plan we
write into the game's own files on export."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
HEADER = ROOT / "vendor/retroarch/rominabox_launch.h"

# The switch for a quiet automated launch, and the opt-out that a person
# testing by hand can set. We take the names from here in harnesses, and in
# the quiet tests we check them against the launcher (test_quiet.plan_check).
QUIET_ENV = "ROMINABOX_QUIET"
SOUND_ENV = "ROMINABOX_SOUND"
# The menu script that we run in the player of a test build. In the launcher
# we also give such a run no controller (checked in the quiet tests).
SCRIPT_ENV = "ROMINABOX_MENU_SCRIPT"

# A launch plan's lines: the game's own storage, its identity, whether it
# runs in a sandbox, and its QUICK SIGN IN folder.
DATA_DIR = re.compile(r'^data_dir\t(.+)$', re.MULTILINE)
IDENTITY = re.compile(r'^identity\t(.+)$', re.MULTILINE)
SANDBOX = re.compile(r'^sandbox\t1$', re.MULTILINE)
ACCOUNTS_DIR = re.compile(r'^accounts_dir\t(.+)$', re.MULTILINE)


def launch_declaration(name: str) -> str:
    """A name the launcher and the player share, from rominabox_launch.h."""
    found = re.search(rf'#define {name} "([^"]+)"', HEADER.read_text(encoding="utf-8"))
    if not found:
        raise SystemExit(f"rominabox_launch.h declares no {name}")
    return found.group(1)


# The variable with a test's own per-user data folder, which we read in the
# launcher in place of the person's. A plan's $user_data is below it.
TEST_USER_DATA_ENV = launch_declaration("RIB_ENV_TEST_USER_DATA")


def read_plan(resources: Path) -> str:
    """The launch plan among a game's own files at `resources`, or empty when
    there is none."""
    path = resources / "launch.plan"
    if path.is_file():
        return path.read_text(encoding="utf-8")
    return ""


def data_dir(plan: str, user_data: Path) -> Path | None:
    """The per-game storage in `plan`, with its $user_data at `user_data`, as we
    compute it in the launcher."""
    found = DATA_DIR.search(plan)
    if not found:
        return None
    return Path(found.group(1).replace("$user_data", str(user_data)))
