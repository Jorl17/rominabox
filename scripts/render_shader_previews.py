"""Render each shader's preview by running the shader, and check it has not drifted.

We make the picture of a filter by applying the filter, so every preset in
the catalogue has a preview, a change to a fragment changes its preview, and
a shader that an author supplies can have a preview too.

    python3 scripts/render_shader_previews.py            # regenerate every preview
    python3 scripts/render_shader_previews.py --check    # fail if any has drifted
    python3 scripts/render_shader_previews.py --library  # redraw libretro's presets

We take the GLSL from the exporter, so we compile the same GLSL as in an
exported game and not a copy of it. Rendering requires the Chrome already on
the machine. We download nothing, and an exported game does not include
build tools.

The libretro presets in the shader library have several passes with textures,
and only RetroArch can run them. We draw their previews with the runtime kit's
own player, with its window hidden. In it we show the same test card through
the preset with RetroArch's built-in image viewer, and the run ends on its
own. The picture depends on the GPU, so we do not redraw these with
`--check`, only with `--library` when the library changes.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PREVIEWS = ROOT / "integrations/shaders/previews"
DRIVER = Path(__file__).resolve().parent / "shader_previews.mjs"
CATALOG = ROOT / "integrations/shaders/catalog.json"
KIT = ROOT / "desktop/src-tauri/resources/runtime"

sys.path.insert(0, str(Path(__file__).resolve().parent))
import scratch  # noqa: E402
from built import cli as _cli  # noqa: E402
from core_source import host_target  # noqa: E402
from native_build import binary_name  # noqa: E402
from programs import windowless  # noqa: E402

# How many times larger we draw the test card in the unfiltered preview.
# Every fourth pixel of that preview is a pixel of the card.
CARD_SCALE = 4
# The video driver we use for each shader language, as in an exported game
# (desktop/src-tauri/src/shader_format.rs).
DRIVERS = {"glsl": "gl", "slang": "glcore"}

# The same tolerances as for the controller artwork, for the same reason. We
# check that it is the same picture, not that two encoders wrote the same
# bytes. Different versions of a software rasteriser round the last bit
# differently.
PIXEL_TOLERANCE = 1.0  # percent of pixels that may differ at all
CHANNEL_TOLERANCE = 6  # how far one of those pixels may be off, per channel


def sources() -> list[dict]:
    """Return every shader that can have a preview, with the source we make it from."""
    # We send `shader-sources` no request, and we close stdin, so that a pipe
    # left open in the test run cannot make us wait or fail with EAGAIN.
    asked = subprocess.run(
        [str(_cli()), "shader-sources"],
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=120,
    )
    if asked.returncode != 0:
        detail = asked.stderr.strip() or asked.stdout.strip()
        raise SystemExit(f"could not ask the exporter for the shader sources: {detail}")
    return json.loads(asked.stdout)["result"]["shaders"]


def draw(shaders: list[dict], destination: Path) -> None:
    with scratch.scratch() as temporary:
        listing = Path(temporary) / "shaders.json"
        listing.write_text(json.dumps(shaders))
        drawn = subprocess.run(
            [
                "node",
                str(DRIVER),
                "--shaders",
                str(listing),
                "--out",
                str(destination),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=300,
        )
        sys.stdout.write(drawn.stdout)
        if drawn.returncode != 0:
            raise SystemExit(drawn.stderr.strip() or "the preview renderer failed")


def library_presets() -> list[tuple[str, str, str]]:
    """Return each libretro preset in the catalogue, with its id, the library
    folder of the language we draw it in, and its path there. We use GLSL
    where the preset has it, as in a game without an author's shader."""
    presets = []
    for preset in json.loads(CATALOG.read_text(encoding="utf-8"))["presets"]:
        files = preset.get("files")
        if files:
            language = "glsl" if "glsl" in files else "slang"
            presets.append((preset["id"], language, files[language]))
    return presets


def card(into: Path) -> Path:
    """Return the test card, taken from the unfiltered preview, which is the
    card drawn larger with no filter."""
    from PIL import Image

    drawn = Image.open(PREVIEWS / "none.png").convert("RGB")
    size = (drawn.width // CARD_SCALE, drawn.height // CARD_SCALE)
    picture = Image.new("RGB", size)
    for y in range(size[1]):
        for x in range(size[0]):
            picture.putpixel((x, y), drawn.getpixel((x * CARD_SCALE + 1, y * CARD_SCALE + 1)))
    path = into / "card.png"
    picture.save(path)
    return path


# Every folder for the player's files, each inside the folder of this run.
PLAYER_FOLDERS = {
    "system_directory": "system",
    "savefile_directory": "saves",
    "savestate_directory": "states",
    "cache_directory": "cache",
    "screenshot_directory": "screenshots",
    "log_dir": "logs",
    "video_shader_dir": "shaders",
    "libretro_info_path": "info",
    "libretro_directory": "cores",
    "playlist_directory": "playlists",
}


def draw_library(destination: Path) -> None:
    """Draw the preview of each libretro preset with the runtime kit's player."""
    from PIL import Image

    player = KIT / "bin" / binary_name(host_target())
    if not player.is_file():
        raise SystemExit(f"no player in the runtime kit: {player}")
    size = Image.open(PREVIEWS / "none.png").size
    with scratch.scratch() as temporary:
        work = Path(temporary)
        test_card = card(work)
        for id, language, path in library_presets():
            data = work / id
            folders = {key: data / name for key, name in PLAYER_FOLDERS.items()}
            for folder in folders.values():
                folder.mkdir(parents=True)
            # We start it without controllers or text, so nothing on this
            # machine appears over the picture or changes it.
            settings = {
                "video_driver": DRIVERS[language],
                "menu_driver": "null",
                "audio_driver": "null",
                "input_joypad_driver": "null",
                "video_gpu_screenshot": "true",
                "video_shader_enable": "true",
                "video_fullscreen": "false",
                "video_scale": f"{CARD_SCALE}.000000",
                "video_smooth": "false",
                "video_font_enable": "false",
                "pause_nonactive": "false",
                "config_save_on_exit": "false",
                "builtin_imageviewer_enable": "true",
                "auto_shaders_enable": "false",
                "history_list_enable": "false",
                **{key: folder.as_posix() for key, folder in folders.items()},
            }
            config = work / f"{id}.cfg"
            config.write_text(
                "".join(f'{key} = "{value}"\n' for key, value in settings.items()),
                encoding="utf-8",
            )
            shot = work / f"{id}.png"
            ran = subprocess.run(
                [
                    str(player), "--config", str(config), "-L", "imageviewer", str(test_card),
                    "--set-shader", str(KIT / "shaders" / language / path),
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
                detail = ran.stderr.strip() or ran.stdout.strip()
                raise SystemExit(f"the player drew no picture of {id}: {detail}")
            picture = Image.open(shot).convert("RGBA")
            if picture.size != size:
                picture = picture.resize(size, Image.Resampling.BOX)
            destination.mkdir(parents=True, exist_ok=True)
            picture.save(destination / f"{id}.png")
            print(f"  drew    {id}.png ({language})")


def compare(recorded: Path, fresh: Path) -> str:
    """Return why the preview does not match, or nothing when it does."""
    from PIL import Image

    a = Image.open(recorded).convert("RGBA")
    b = Image.open(fresh).convert("RGBA")
    if a.size != b.size:
        return f"size {b.size} against {a.size}"
    differing = 0
    for p, q in zip(a.getdata(), b.getdata()):
        if max(abs(x - y) for x, y in zip(p, q)) > CHANNEL_TOLERANCE:
            differing += 1
    percent = 100 * differing / (a.size[0] * a.size[1])
    if percent > PIXEL_TOLERANCE:
        return f"{percent:.1f}% of pixels differ, which is more than rounding"
    return ""


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare the recorded previews against a fresh render and fail on drift",
    )
    parser.add_argument(
        "--library",
        action="store_true",
        help="redraw the previews of libretro's presets with the runtime kit's player",
    )
    arguments = parser.parse_args()

    if arguments.library:
        draw_library(PREVIEWS)
        print(f"\nrendered {len(library_presets())} library previews -> {PREVIEWS}")
        return 0

    shaders = sources()
    if not shaders:
        raise SystemExit("the exporter offers no shaders, so there is nothing to draw")

    if not arguments.check:
        draw(shaders, PREVIEWS)
        print(f"\nrendered {len(shaders)} shader previews -> {PREVIEWS}")
        return 0

    with scratch.scratch() as temporary:
        fresh = Path(temporary)
        draw(shaders, fresh)
        drifted: list[str] = []
        for shader in shaders:
            name = f"{shader['id']}.png"
            recorded = PREVIEWS / name
            if not recorded.is_file():
                print(f"  MISSING {name}", file=sys.stderr)
                drifted.append(shader["id"])
                continue
            verdict = compare(recorded, fresh / name)
            if verdict:
                print(f"  DRIFTED {name}: {verdict}", file=sys.stderr)
                drifted.append(shader["id"])
            else:
                print(f"  ok      {name}")
        extra = sorted(
            path.name
            for path in PREVIEWS.glob("*.png")
            if path.stem not in {shader["id"] for shader in shaders}
            and path.stem not in {id for id, _, _ in library_presets()}
        )
        for name in extra:
            print(f"  ORPHAN  {name}: no shader in the catalogue draws this", file=sys.stderr)
        if drifted or extra:
            print(
                f"\n{len(drifted) + len(extra)} preview(s) do not match the shaders "
                "that make them. The shader is the source. Regenerate with:\n"
                "  python3 scripts/render_shader_previews.py",
                file=sys.stderr,
            )
            return 1
        print(f"\nall {len(shaders)} previews match the shader that makes them")
        return 0


if __name__ == "__main__":
    sys.exit(main())
