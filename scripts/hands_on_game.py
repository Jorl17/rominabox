"""Export games for a person to try by hand, as we make them in the builder:
we look up each ROM online and give it the builder's settings, as in the
exporter's `export` for a ROM alone, and use a player built from the fork
as it is now.

    uv run python scripts/hands_on_game.py [--shaders ID,ID...] PLAYER_BUILD OUTPUT ROM...

With --shaders we bundle those shaders of the catalog (integrations/shaders/catalog.json)
in every game, which then has a Filters screen. PLAYER_BUILD is a player we ship (scripts/build_player.py with no test
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


def export(request: dict) -> dict:
    """The result of one export, or the reason it failed."""
    done = subprocess.run([str(menu_shots.command()), "export"], input=json.dumps(request),
                          capture_output=True, text=True, encoding="utf-8")
    events = [json.loads(line) for line in done.stdout.splitlines() if line.startswith("{")]
    last = events[-1] if events else {}
    if last.get("type") != "result":
        raise SystemExit(f"export failed: {last or done.stderr[-500:]}")
    return last["result"]


def main() -> int:
    arguments = sys.argv[1:]
    shaders = []
    if arguments[:1] == ["--shaders"] and len(arguments) > 1:
        shaders = [shader for shader in arguments[1].split(",") if shader]
        arguments = arguments[2:]
    if len(arguments) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    build = player_build.current_build(Path(arguments[0]))
    if player_build.info(build).get("testOnly"):
        raise SystemExit(f"{build} is a test build; a person tries the player that ships")
    output = Path(arguments[1]).resolve()
    if output.exists():
        raise SystemExit(f"refusing to write into existing {output}")
    roms = [Path(rom) for rom in arguments[2:]]
    missing = [str(rom) for rom in roms if not rom.is_file()]
    if missing:
        raise SystemExit(f"no ROM at {', '.join(missing)}")
    output.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="rominabox-hands-on-") as staging:
        kit = menu_shots.staged_kit(Path(staging) / "kit", player_build.player_in(build))
        for rom in roms:
            # The kit with this player in it, and this checkout's cores. The
            # rest is as in the builder.
            made = export({"rom": str(rom), "outputDir": str(output), "runtimeKit": str(kit),
                           "coreCache": str(core_source()), "shaders": {"bundled": shaders}})
            print(f"{rom.name}: {made['appPath']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
