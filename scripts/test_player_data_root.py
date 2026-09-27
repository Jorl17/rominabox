"""Check that in a game's player we use only its data folder, and refuse to start without one.

Stock RetroArch fails open. With no folder given, the Windows build created
about 25 folders beside the program and would read the user's configuration.
A game must only use the absolute data folder that we give in
ROMINABOX_DATA_DIR in its launcher. We ask the player only for its feature
list, which we print before any window or core, so we open no window here.

    ROMINABOX_TEST_BUILD=/absolute/build python3 scripts/test_player_data_root.py
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import player_build  # noqa: E402

REFUSAL = "ROMINABOX_DATA_DIR must be set to an absolute path"


def ask_features(player: Path, data_dir: str | None) -> subprocess.CompletedProcess:
    environment = {name: value for name, value in os.environ.items() if not name.startswith("ROMINABOX_")}
    if data_dir is not None:
        environment["ROMINABOX_DATA_DIR"] = data_dir
    return subprocess.run([str(player), "--features"], cwd=player.parent, env=environment,
                          capture_output=True, text=True, errors="replace", timeout=60)


def main() -> int:
    source = player_build.player_in(player_build.selected_build())
    failures = 0
    with tempfile.TemporaryDirectory(prefix="rominabox-data-root-") as temporary:
        # The player alone in a folder, so that we see anything created beside it.
        folder = Path(temporary) / "bin"
        folder.mkdir()
        player = folder / source.name
        shutil.copy2(source, player)
        data = Path(temporary) / "data"
        data.mkdir()

        for label, data_dir in (("no data folder", None), ("a relative data folder", "data")):
            ran = ask_features(player, data_dir)
            output = ran.stdout + ran.stderr
            beside = sorted(entry.name for entry in folder.iterdir() if entry != player)
            if ran.returncode == 0 or REFUSAL not in output:
                print(f"FAIL with {label} the player started (exit {ran.returncode}):\n{output[-1500:]}")
                failures += 1
            elif beside:
                print(f"FAIL with {label} the player refused but created {beside} beside itself")
                failures += 1
            else:
                print(f"ok with {label} the player refused and created nothing")

        ran = ask_features(player, str(data))
        beside = sorted(entry.name for entry in folder.iterdir() if entry != player)
        if ran.returncode != 0 or REFUSAL in ran.stdout + ran.stderr:
            print(f"FAIL with an absolute data folder the player did not start (exit {ran.returncode}):\n"
                  f"{(ran.stdout + ran.stderr)[-1500:]}")
            failures += 1
        elif beside:
            # A game's folders are in its data folder, and there is nothing of
            # RetroArch's usual layout beside the program.
            print(f"FAIL with an absolute data folder the player created {beside} beside itself")
            failures += 1
        else:
            made = sorted(entry.name for entry in data.iterdir())
            print(f"ok with an absolute data folder the player starts, and made {made} there and nothing beside itself")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
