"""Export games for a person to try by hand, as we would make them in the
builder: with each ROM's title, console, description and cover from an online
lookup, the Native design, achievements, and a player built from the fork now.

    python3 scripts/hands_on_game.py PLAYER_BUILD OUTPUT ROM...

PLAYER_BUILD is a player we ship (scripts/build_player.py with no test
switch). We put it in place of the kit's player, as we do with a test player
in the launched tests (menu_shots.staged_kit). The games go into OUTPUT, which
must not exist yet. We launch nothing.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import menu_shots  # noqa: E402
import player_build  # noqa: E402
from core_source import core_source  # noqa: E402


def run(command: str, request: dict) -> dict:
    """The result of one exporter command, or the reason it failed."""
    done = subprocess.run([str(menu_shots.command()), command], input=json.dumps(request),
                          capture_output=True, text=True, encoding="utf-8")
    events = [json.loads(line) for line in done.stdout.splitlines() if line.startswith("{")]
    last = events[-1] if events else {}
    if last.get("type") != "result":
        raise SystemExit(f"{command} failed: {last or done.stderr[-500:]}")
    return last["result"]


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__, file=sys.stderr)
        return 2
    build = player_build.current_build(Path(sys.argv[1]))
    if player_build.info(build).get("testOnly"):
        raise SystemExit(f"{build} is a test build; a person tries the player that ships")
    output = Path(sys.argv[2]).resolve()
    if output.exists():
        raise SystemExit(f"refusing to write into existing {output}")
    roms = [Path(rom) for rom in sys.argv[3:]]
    missing = [str(rom) for rom in roms if not rom.is_file()]
    if missing:
        raise SystemExit(f"no ROM at {', '.join(missing)}")
    output.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="rominabox-hands-on-") as staging:
        kit = menu_shots.staged_kit(Path(staging) / "kit", player_build.player_in(build))
        for rom in roms:
            found = run("inspect", {"rom": str(rom), "cache": str(output / "lookup"), "online": True})
            made = run("export", {
                "rom": str(rom), "title": found["title"], "system": found["system"],
                "description": found.get("description") or "", "icon": found.get("iconPath"),
                "background": None, "showMenu": True, "startAtMenu": False, "theme": "native",
                "palette": "blue", "splash": True, "includeAchievements": True,
                "keepPlayingInBackground": False, "autosaveOnQuit": False,
                "advancedEmulatorAccess": False, "outputDir": str(output),
                "target": menu_shots.PLATFORM, "runtimeKit": str(kit), "core": None,
                "coreCache": str(core_source()),
            })
            print(f"{found['title']} ({found['system']}): {made['appPath']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
