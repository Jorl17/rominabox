"""Check that the shader row marked ON is the preset the game is running.

When the player chooses a shader, we apply it, and on the next launch we pass
the same preset to RetroArch. If we marked the row from the author's starting
preset when we bundle the game, ON would stay on that row after a restart
while a different filter was running.

We export our generated Game Boy Color cartridge with two presets, with the
author's choice on the second. In the first launch we choose the other one.
In the second launch we open the shader screen, and the row marked ON (the
one with the `selected` class, which the design draws as ON) must be the
preset running in that launch. We read the marked row from the player's
checkpoint report, and keep the picture of the menu for a person to look at.

    uv run python scripts/shader_state.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import exported_game  # noqa: E402
import menu_shots  # noqa: E402
import toolchain  # noqa: E402
from menu_workflows import checkpoints  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
# Our own committed cartridge. Any game in which we can open the shader screen will do.
ROM = ROOT / "scripts/fixtures/test-game.gbc"
BUILD = ROOT / "work/shaderstate-build"
SHOTS = ROOT / "work/shaderstate-shots"
MARK_SOURCE = ROOT / "scripts/native_runtime/test_shader_mark.c"
MARK_BINARY = toolchain.executable(ROOT / "work/shaderstate-mark")

CHOSEN = "scanlines"
INITIAL = "phosphor"


def shader_config(app: Path) -> dict[str, str]:
    """Return a map from id to preset path, in the order we stage the list."""
    path = exported_game.resources_of(app) / "menu-assets/shaders.cfg"
    text = path.read_text(encoding="utf-8")
    ids: list[str] = []
    presets: dict[str, str] = {}
    for line in text.splitlines():
        if line.startswith("shader_ids = "):
            ids = line.split('"', 2)[1].split()
        elif line.startswith("shader_preset_"):
            key, _, value = line.partition(" = ")
            presets[key.removeprefix("shader_preset_")] = value.strip().strip('"')
    if not ids:
        raise SystemExit(f"{path} names no shaders")
    return {shader_id: presets.get(shader_id, "") for shader_id in ids}


def running_id(app: Path, listed: dict[str, str]) -> str:
    """Return the listed filter that we will start the game with.

    The choice file contains the filter's id. At the next launch we look it
    up in the game's shaders.cfg and pass that preset as --set-shader. We
    read the file to know which filter is running, because we must not keep
    a second copy in the menu and read that instead.
    """
    data = exported_game.data_dir_of(app)
    if data is None:
        raise SystemExit("the export has no data directory")
    choice = data / "shader-choice"
    if not choice.is_file():
        raise SystemExit(
            "choosing a shader did not leave a preset for the next launch"
        )
    # In the menu we save the chosen filter's id, and at launch we find its preset.
    current = choice.read_text(encoding="utf-8").strip()
    matched = current if current in listed else None
    if matched is None:
        raise SystemExit(f"the saved choice names no bundled shader: {current!r}")
    return matched


def shoot(app: Path, name: str, script: list[str]) -> None:
    SHOTS.mkdir(parents=True, exist_ok=True)
    problem = menu_shots.take(app, name, script, SHOTS)
    if problem:
        raise SystemExit(f"{name}: {problem}")


def unit() -> None:
    """The row index is a pure function of the running preset path."""
    compiled = subprocess.run(
        [
            toolchain.describe()["cc"],
            "-Wall",
            "-Werror",
            "-I",
            str(ROOT / "vendor/retroarch/menu/drivers"),
            "-I",
            str(ROOT / "vendor/retroarch/libretro-common/include"),
            "-o",
            str(MARK_BINARY),
            str(MARK_SOURCE),
        ],
        capture_output=True,
        text=True,
    )
    if compiled.returncode != 0:
        raise SystemExit(compiled.stderr.strip() or "could not compile the shader mark check")
    checked = subprocess.run([str(MARK_BINARY)], capture_output=True, text=True)
    if checked.returncode != 0:
        raise SystemExit(checked.stderr.strip() or "shader mark check failed")


def main() -> int:
    unit()
    if not ROM.is_file():
        raise SystemExit(f"the generated cartridge is not at {ROM}")
    with menu_shots.build_a_game(
        ROM,
        BUILD,
        system="gbc",
        settings={
            "shaders": {
                "bundled": ["scanlines", "phosphor"],
                "initial": INITIAL,
            },
        },
    ) as app:
        listed = shader_config(app)
        if list(listed)[:3] != ["none", "scanlines", "phosphor"]:
            raise SystemExit(f"unexpected shader list: {list(listed)}")
        if INITIAL not in listed or CHOSEN not in listed:
            raise SystemExit(f"the export is missing a shader: {list(listed)}")

        data = exported_game.data_dir_of(app)
        if data is not None:
            choice = data / "shader-choice"
            if choice.is_file():
                choice.unlink()

        shoot(app, "chosen", ["options", "shaders", CHOSEN])
        running = running_id(app, listed)
        if running != CHOSEN:
            raise SystemExit(
                f"the first launch was asked for {CHOSEN} and saved {running}"
            )

        shoot(app, "restarted", ["options", "shaders", "report:restarted"])
    marked = checkpoints(SHOTS / "restarted.log")["restarted"]["selected"]
    print(f"running {running}")
    print(f"marked  {marked}  picture={SHOTS / 'restarted.png'}")
    if len(marked) != 1:
        raise SystemExit(f"the shader screen marks {marked}, not one row")
    shown = marked[0]
    if shown != running:
        raise SystemExit(
            f"the shader that is running is {running}, "
            f"the row marked ON is {shown}"
        )
    print("the row marked ON is the shader that is running")
    return 0


if __name__ == "__main__":
    sys.exit(main())
