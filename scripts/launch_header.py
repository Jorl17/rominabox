"""Names the launcher and the player share, read from the fork's
rominabox_launch.h, so we spell each the same way in a harness."""

from __future__ import annotations

import re
from pathlib import Path

HEADER = Path(__file__).resolve().parent.parent / "vendor/retroarch/rominabox_launch.h"


def launch_declaration(name: str) -> str:
    """A name the launcher and the player share, from rominabox_launch.h."""
    found = re.search(rf'#define {name} "([^"]+)"', HEADER.read_text(encoding="utf-8"))
    if not found:
        raise SystemExit(f"rominabox_launch.h declares no {name}")
    return found.group(1)


# The variable with a test's own per-user data folder, which we read in the
# launcher in place of the person's. A plan's $user_data is below it.
TEST_USER_DATA_ENV = launch_declaration("RIB_ENV_TEST_USER_DATA")
