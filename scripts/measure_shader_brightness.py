"""Measure the light at each value of the brightness parameter of each bundled shader.

In the catalogue (integrations/shaders/catalog.json) we can give a preset its
brightness parameter, as `"brightness": {"parameter": ...}`. For each such
preset we draw the test card through the preset with the test player, with
its window hidden, at the value of the parameter in the preset and at values
up to the parameter's maximum. We write the light of each picture, as a
multiple of the light at the value in the preset, into the preset's `table`,
for as long as the light still rises. In a game, for a brightness above
100 %, we raise this parameter first and add the rest of the light with our
pass (desktop/crates/rominabox-engine/src/video.rs).

    uv run python scripts/measure_shader_brightness.py

We measure the slang version of a preset where there is one, because on a
Mac we cannot run the GLSL of some presets with the gl driver. A table
contains the light of the pictures from one GPU and one card, so we keep it
as measured and do not check it in a test.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import player_build  # noqa: E402
import render_shader_previews as previews  # noqa: E402
from programs import windowless  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
LIBRARY = ROOT / "integrations/shaders/library"
# The values we draw between the value in the preset and the maximum.
STEPS = 6
# We count a rise in light smaller than this as none, and end the table there.
LEAST_RISE = 0.01


def light(picture: Path) -> float:
    """The mean light of `picture`, in linear light."""
    from PIL import Image

    def linear(value: int) -> float:
        value /= 255.0
        return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4

    table = [linear(value) for value in range(256)]
    pixels = list(Image.open(picture).convert("RGB").get_flattened_data())
    return sum(table[r] + table[g] + table[b] for r, g, b in pixels) / (3 * len(pixels))


def declared(preset: Path, parameter: str) -> tuple[float, float]:
    """The value of `parameter` in the preset and the parameter's maximum,
    from the preset and the `#pragma parameter` line in its passes."""
    text = preset.read_text(encoding="utf-8", errors="replace")
    for name in re.findall(r'^\s*shader\d+\s*=\s*"?([^"\n]+?)"?\s*$', text, re.M):
        source = (preset.parent / name).read_text(encoding="utf-8", errors="replace")
        found = re.search(
            rf'#pragma parameter\s+{parameter}\s+"[^"]*"\s+([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)', source
        )
        if found:
            initial, maximum = float(found.group(1)), float(found.group(3))
            own = re.search(rf'^\s*{parameter}\s*=\s*"?([-\d.]+)"?', text, re.M)
            return (float(own.group(1)) if own else initial), maximum
    raise SystemExit(f"{preset} declares no parameter {parameter}")


def draw(player: Path, card: Path, preset: Path, language: str, work: Path, name: str) -> Path:
    """The picture we take of the test card through `preset` with the player."""
    data = work / name
    folders = {key: data / folder for key, folder in previews.PLAYER_FOLDERS.items()}
    for folder in folders.values():
        folder.mkdir(parents=True, exist_ok=True)
    settings = {
        "video_driver": previews.DRIVERS[language],
        "menu_driver": "null",
        "audio_driver": "null",
        "input_joypad_driver": "null",
        "video_gpu_screenshot": "true",
        "video_shader_enable": "true",
        "video_fullscreen": "false",
        "video_scale": f"{previews.CARD_SCALE}.000000",
        "video_smooth": "false",
        "video_font_enable": "false",
        "pause_nonactive": "false",
        "config_save_on_exit": "false",
        "builtin_imageviewer_enable": "true",
        "auto_shaders_enable": "false",
        "history_list_enable": "false",
        **{key: folder.as_posix() for key, folder in folders.items()},
    }
    config = work / f"{name}.cfg"
    config.write_text("".join(f'{key} = "{value}"\n' for key, value in settings.items()), encoding="utf-8")
    shot = work / f"{name}.png"
    ran = subprocess.run(
        [
            str(player), "--config", str(config), "-L", "imageviewer", str(card),
            "--set-shader", str(preset),
            "--max-frames=30", "--max-frames-ss", f"--max-frames-ss-path={shot}",
        ],
        env=dict(os.environ, ROMINABOX_QUIET="1", ROMINABOX_DATA_DIR=str(data)),
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=120,
        **windowless(),
    )
    if ran.returncode != 0 or not shot.is_file():
        raise SystemExit(f"the player drew no picture of {name}: {(ran.stderr or ran.stdout).strip()[-800:]}")
    return shot


def measure(player: Path, card: Path, preset: dict, work: Path) -> list[tuple[float, float]]:
    """The table of `preset`, as (light, value) pairs from the value in the preset up."""
    files = preset["files"]
    language = "slang" if files.get("slang") else "glsl"
    library_preset = LIBRARY / language / files[language]
    parameter = preset["brightness"]["parameter"]
    own, maximum = declared(library_preset, parameter)
    table: list[tuple[float, float]] = []
    base = None
    for step in range(STEPS + 1):
        value = round(own + (maximum - own) * step / STEPS, 4)
        # A preset with a `#reference` line for the library's preset and a
        # value for the parameter.
        varied = work / f"{preset['id']}-{step}{library_preset.suffix}"
        varied.write_text(f'#reference "{library_preset.as_posix()}"\n{parameter} = "{value}"\n', encoding="utf-8")
        measured = light(draw(player, card, varied, language, work, f"{preset['id']}-{step}"))
        base = base or measured
        ratio = round(measured / base, 3)
        if table and ratio < table[-1][0] + LEAST_RISE:
            break
        table.append((ratio, value))
        print(f"  {preset['id']}: {parameter} = {value} gives {ratio}x the light")
    return table


def main() -> int:
    player = player_build.player_in(player_build.selected_build())
    catalog = json.loads(previews.CATALOG.read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="rominabox-shader-brightness-") as temporary:
        work = Path(temporary)
        card = previews.card(work)
        for preset in catalog["presets"]:
            if "brightness" in preset:
                preset["brightness"]["table"] = measure(player, card, preset, work)
    previews.CATALOG.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
