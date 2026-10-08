"""Measure the light at each value of the brightness parameter of each bundled shader.

In the catalogue (integrations/shaders/catalog.json) we can give a preset its
brightness parameter, as `"brightness": {"parameter": ...}`. For each such
preset we draw the test card through the preset with the test player, with
its window hidden, at the value of the parameter in the preset and at values
up to the parameter's maximum. We write the light of each picture, as a
multiple of the light at the value in the preset, into the preset's `table`,
for as long as the light still rises and until it reaches the highest
BRIGHTNESS a player can choose (settings.inc). In a game, for a brightness
above 100 %, we raise this parameter first and add the rest of the light with
our pass (desktop/crates/rominabox-engine/src/video.rs).

    uv run python scripts/measure_shader_brightness.py
    uv run python scripts/measure_shader_brightness.py --language glsl --check

We measure the slang version of a preset where there is one, because on a
Mac we cannot run the GLSL of some presets with the gl driver (`#version
130`). A table contains the light of the pictures from one GPU and one card,
so we keep it as measured and do not check it in a test. In a game in which
no shader is only in slang, we run the GLSL of each preset. With `--check` we
write nothing. We measure the presets in `--language` and print how far
their light is from the table in the catalogue at each value, so that on
Windows, where we can run that GLSL with the gl driver, someone can confirm
that one table is right for both languages.
"""

from __future__ import annotations

import argparse
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
SETTINGS = ROOT / "vendor/retroarch/menu/drivers/rmlui/settings.inc"
# The most values we draw above the value in the preset. In the game we read
# at most 16 rows of a table (RIB_VIDEO_CONTROL_POSITIONS in video.c).
STEPS = 15
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


def top_brightness() -> float:
    """The highest BRIGHTNESS a player can choose, from settings.inc."""
    text = SETTINGS.read_text(encoding="utf-8")
    found = re.search(r'RIB_SETTING_POSITIONS\(VideoBrightness,\s*"([^"]+)"\)', text)
    if not found:
        raise SystemExit(f"{SETTINGS} gives no positions for VideoBrightness")
    return max(float(value) for value in found.group(1).split())


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def included(source: Path) -> list[Path]:
    """`source`, the files in its `#include` lines and the files in theirs."""
    found = [source]
    for name in re.findall(r'^\s*#include\s+"([^"]+)"', read(source), re.M):
        found += included((source.parent / name).resolve())
    return found


def files(preset: Path) -> tuple[list[Path], list[Path]]:
    """The presets, `preset` first and then those in its `#reference` lines,
    and the shader sources of their passes."""
    text = read(preset)
    presets, sources = [preset], []
    for name in re.findall(r'^\s*#reference\s+"?([^"\n]+?)"?\s*$', text, re.M):
        more_presets, more_sources = files((preset.parent / name).resolve())
        presets += more_presets
        sources += more_sources
    # A path may end in two quotes, as in libretro's GLSL ntsc/blargg.glslp,
    # and RetroArch reads it all the same.
    for name in re.findall(r'^\s*shader\d+\s*=\s*"?([^"\n]+?)"*\s*$', text, re.M):
        sources += included((preset.parent / name).resolve())
    return presets, sources


def declared(preset: Path, parameter: str) -> tuple[float, float]:
    """The value of `parameter` in the preset and the parameter's maximum,
    from the presets and the `#pragma parameter` line in the shader sources."""
    presets, sources = files(preset)
    name = re.escape(parameter)
    for source in sources:
        found = re.search(rf'#pragma parameter\s+{name}\s+"[^"]*"\s+([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)', read(source))
        if found:
            initial, maximum = float(found.group(1)), float(found.group(3))
            for text in map(read, presets):
                own = re.search(rf'^\s*{name}\s*=\s*"?([-\d.]+)"?', text, re.M)
                if own:
                    return float(own.group(1)), maximum
            return initial, maximum
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


def measure(player: Path, card: Path, preset: dict, language: str, work: Path) -> list[tuple[float, float]]:
    """The table of `preset` in `language`, as (light, value) pairs from the
    value in the preset up."""
    library_preset = LIBRARY / language / preset["files"][language]
    parameter = preset["brightness"]["parameter"]
    own, maximum = declared(library_preset, parameter)
    top = top_brightness()
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
        if ratio >= top:
            break
    return table


def compare(preset: dict, table: list[tuple[float, float]]) -> str:
    """How far the light in `table` is from the table of `preset` in the
    catalogue, at the values in both."""
    recorded = {value: light for light, value in preset["brightness"]["table"]}
    differences = [abs(light - recorded[value]) for light, value in table if value in recorded]
    if len(differences) < 2:
        return f"{preset['id']}: rises to {table[-1][0]}x, the table to {max(recorded.values())}x"
    return f"{preset['id']}: at most {max(differences):.3f}x apart over {len(differences)} values"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--language", choices=["slang", "glsl"],
                        help="the language to measure; by default slang where a preset has it")
    parser.add_argument("--check", action="store_true",
                        help="compare with the catalogue's tables and write nothing")
    arguments = parser.parse_args()
    player = player_build.player_in(player_build.selected_build())
    catalog = json.loads(previews.CATALOG.read_text(encoding="utf-8"))
    report = []
    with tempfile.TemporaryDirectory(prefix="rominabox-shader-brightness-") as temporary:
        work = Path(temporary)
        card = previews.card(work)
        for preset in catalog["presets"]:
            if "brightness" not in preset:
                continue
            files = preset["files"]
            language = arguments.language or ("slang" if files.get("slang") else "glsl")
            if not files.get(language):
                continue
            table = measure(player, card, preset, language, work)
            if arguments.check:
                report.append(compare(preset, table))
            elif len(table) > 1:
                preset["brightness"]["table"] = table
            else:
                # Raising the parameter does not change the light, so we add
                # all of the light with our pass.
                print(f"  {preset['id']}: {preset['brightness']['parameter']} adds no light")
                del preset["brightness"]
    if arguments.check:
        print("\n".join(report))
    else:
        previews.CATALOG.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
