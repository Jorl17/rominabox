"""Check that the shader row marked ON is the preset the game is running.

When the player chooses a shader, we apply it, and on the next launch we pass
the same preset to RetroArch. If we marked the row from the author's starting
preset when we bundle the game, ON would stay on that row after a restart
while a different filter was running.

We export our generated Game Boy Color cartridge with two presets, with the
author's choice on the second. In the first launch we choose the other one.
In the second launch we open the shader screen, and the row marked ON must
be the preset running in that launch. We take the picture of the menu from
inside the game.

    python3 scripts/shader_state.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import menu_shots  # noqa: E402
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
# Our own committed cartridge. Any game in which we can open the shader screen will do.
ROM = ROOT / "scripts/fixtures/test-game.gbc"
BUILD = ROOT / "work/shaderstate-build"
SHOTS = ROOT / "work/shaderstate-shots"
MARK_SOURCE = ROOT / "scripts/native_runtime/test_shader_mark.c"
MARK_BINARY = toolchain.executable(ROOT / "work/shaderstate-mark")

# The blue palette's highlight. We draw ON in it, and nothing for an empty state.
HIGHLIGHT = (255, 241, 61)
CHOSEN = "scanlines"
INITIAL = "phosphor"


def shader_config(app: Path) -> dict[str, str]:
    """Return a map from id to preset path, in the order we stage the list."""
    path = menu_shots.resources_of(app) / "menu-assets/shaders.cfg"
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
    """Return the listed preset that we will pass to RetroArch at launch.

    At the next launch we pass the choice file as --set-shader. We match a row
    to it to know which filter is running, because we must not keep a second
    copy in the menu and read that instead.
    """
    data = menu_shots.data_dir_of(app)
    if data is None:
        raise SystemExit("the export has no data directory")
    choice = data / "shader-choice"
    if not choice.is_file():
        raise SystemExit(
            "choosing a shader did not leave a preset for the next launch"
        )
    current = choice.read_text(encoding="utf-8").strip()
    if not current:
        matched = next((shader_id for shader_id, preset in listed.items() if not preset), None)
    else:
        matched = next(
            (
                shader_id
                for shader_id, preset in listed.items()
                if preset and (current == preset or current.endswith("/" + preset))
            ),
            None,
        )
    if matched is None:
        raise SystemExit(f"the saved preset matches no bundled shader: {current}")
    return matched


def _near(pixel: tuple[int, ...], colour: tuple[int, int, int]) -> bool:
    return all(abs(channel - wanted) <= 28 for channel, wanted in zip(pixel[:3], colour))


def marked_row(image_path: Path, count: int) -> tuple[int, list[int]]:
    """Return the row whose state column is in the highlight colour.

    The window is 960 by 600, and the list is inside the screen frame. The
    state words are in the right-hand column of each row. ON is in the
    highlight colour and that column is empty for an off row, so the row with
    the highlight there is the one marked ON.
    """
    from PIL import Image

    image = Image.open(image_path).convert("RGB")
    width, _height = image.size
    scale = width / 960
    # #screen is border-box at the origin of a 960-wide window, with a 4dp
    # frame. The list is 36dp in and 68dp down inside that frame.
    list_left = (4 + 36) * scale
    list_top = (4 + 68) * scale
    list_width = 880 * scale
    row_height = 68 * scale
    gap = 4 * scale
    state_right = list_left + list_width - 14 * scale
    state_left = state_right - 110 * scale
    scores: list[int] = []
    pixels = image.load()
    for index in range(count):
        top = int(list_top + index * (row_height + gap) + 8 * scale)
        bottom = int(top + row_height - 16 * scale)
        left = int(state_left)
        right = int(state_right)
        score = 0
        for y in range(max(top, 0), min(bottom, image.size[1])):
            for x in range(max(left, 0), min(right, image.size[0])):
                if _near(pixels[x, y], HIGHLIGHT):
                    score += 1
        scores.append(score)
    return max(range(count), key=lambda index: scores[index]), scores


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

        data = menu_shots.data_dir_of(app)
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

        shoot(app, "restarted", ["options", "shaders"])
    picture = SHOTS / "restarted.png"
    index, scores = marked_row(picture, len(listed))
    shown = list(listed)[index]
    print(f"running {running}")
    print(f"marked  {shown}  scores={scores}  picture={picture}")
    if max(scores) < 8:
        raise SystemExit(
            f"no ON mark was visible in {picture} (scores {scores})"
        )
    if shown != running:
        raise SystemExit(
            f"the shader that is running is {running}, "
            f"the row marked ON is {shown}"
        )
    print("the row marked ON is the shader that is running")
    return 0


if __name__ == "__main__":
    sys.exit(main())
