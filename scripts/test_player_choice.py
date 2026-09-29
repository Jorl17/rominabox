"""Check which player build we run in a launched test when none is given. It
is the newest one built from the current fork, with the menu's script driver
and with its player present. We never take a newer build of another fork
commit, because its player does not match the current fork.

    python3 scripts/test_player_choice.py
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import core_source  # noqa: E402
import native_build  # noqa: E402
import player_build  # noqa: E402
import scratch  # noqa: E402

CURRENT, OTHER = "b" * 40, "a" * 40


def build(root: Path, name: str, revision: str, made_at: int,
          script: bool = True, player: bool = True) -> Path:
    """Make a player build folder as scripts/build_player.py leaves it."""
    folder = root / name
    (folder / "retroarch").mkdir(parents=True)
    (folder / "build-info.json").write_text(json.dumps({
        "retroarchCommit": revision,
        "target": core_source.host_target(),
        "capabilities": {"menuScript": script},
    }))
    if player:
        binary = folder / "retroarch" / native_build.binary_name(core_source.host_target())
        binary.write_bytes(b"player")
        os.utime(binary, (made_at, made_at))
    return folder


def main() -> int:
    failures = []
    with scratch.scratch("rominabox-player-choice-") as made:
        root = Path(made)
        current = build(root, "current", CURRENT, 1000)
        build(root, "newer-other-commit", OTHER, 2000)
        build(root, "newer-without-script-driver", CURRENT, 3000, script=False)
        build(root, "newer-without-player", CURRENT, 4000, player=False)
        chosen = player_build.newest_script_build(root.iterdir(), CURRENT)
        if chosen != current:
            failures.append(f"chose {chosen}, not the only usable build of the current commit")
        if player_build.newest_script_build(root.iterdir(), "c" * 40) is not None:
            failures.append("chose a build when none is of the current commit")
        later = build(root, "current-later", CURRENT, 5000)
        if player_build.newest_script_build(root.iterdir(), CURRENT) != later:
            failures.append("of two builds of the current commit, did not choose the newer")
    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print("player choice: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
