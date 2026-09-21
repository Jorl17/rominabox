"""Render each illustrated controller profile as the player draws it.

This is a tool for visual regression. We read the same declarations as the
exporter and reproduce the RmlUi overlay geometry exactly, so a change that
moves a button anchor, a callout or an illustration changes the image.

We copied this geometry from the shipped code.
  - the scene is 960x380 dp and the illustration fills it
    (desktop/assets/menu/menu.rcss #controller-scene / #controller-image)
  - a control's (x, y) is the button centre, and the hit marker is a 42 dp
    circle (.control-hit is border-radius 21dp)
  - a callout is 196x54 dp at (calloutX, calloutY)
  - the leader is an L, horizontal at calloutY+28 from the callout's inner
    edge across to the button's x, then vertical to the button's y
    (desktop/src-tauri/src/themes.rs prepare_controls_assets)

The pictures are at 2x, the scale of the shipped ones.

    python3 scripts/render_control_overlays.py OUTPUT_DIR
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
CONTROLS = ROOT / "desktop/controls.json"
ARTWORK = ROOT / "desktop/assets/controllers"
FONT = ROOT / "desktop/assets/menu/Silkscreen-Regular.ttf"

SCENE = (960, 380)
SCALE = 2
LEADER = (180, 230, 255, 255)
MARKER = (255, 255, 255, 255)
CALLOUT_FILL = (6, 26, 72, 255)
CALLOUT_EDGE = (117, 178, 228, 255)
ASSIGNMENT = (180, 230, 255, 255)
STAGE = (32, 36, 44, 255)


def _font(size: int) -> ImageFont.ImageFont:
    if FONT.exists():
        try:
            return ImageFont.truetype(str(FONT), size)
        except OSError:
            pass
    return ImageFont.load_default()


def render(profile: dict, destination: Path) -> Path:
    """Draw one profile's overlay over its illustration."""
    image = ARTWORK / profile["image"]
    if not image.exists():
        raise FileNotFoundError(f"{profile['id']} declares {profile['image']}, which is missing")

    canvas = Image.new("RGBA", (SCENE[0] * SCALE, SCENE[1] * SCALE), STAGE)
    canvas.alpha_composite(Image.open(image).convert("RGBA"))
    draw = ImageDraw.Draw(canvas)

    def s(value: float) -> int:
        return round(value * SCALE)

    for control in profile["controls"]:
        x, y = control["x"], control["y"]
        callout_x, callout_y = control["calloutX"], control["calloutY"]
        # The callout's inner edge is its right side in the left gutter and its
        # left side in the right gutter.
        edge = callout_x + 200 if callout_x < 400 else callout_x
        draw.rectangle(
            [s(min(edge, x)), s(callout_y + 28) - 1, s(max(edge, x)), s(callout_y + 28) + 1],
            fill=LEADER,
        )
        draw.rectangle(
            [s(x) - 1, s(min(callout_y + 28, y)), s(x) + 1, s(max(callout_y + 28, y))],
            fill=LEADER,
        )
        draw.ellipse([s(x - 21), s(y - 21), s(x + 21), s(y + 21)], outline=MARKER, width=3)

    for control in profile["controls"]:
        callout_x, callout_y = control["calloutX"], control["calloutY"]
        draw.rectangle(
            [s(callout_x), s(callout_y), s(callout_x + 196), s(callout_y + 54)],
            fill=CALLOUT_FILL,
            outline=CALLOUT_EDGE,
            width=3,
        )
        draw.text((s(callout_x + 10), s(callout_y + 5)), control["label"], font=_font(s(18)), fill=MARKER)
        draw.text((s(callout_x + 10), s(callout_y + 30)), control["key"], font=_font(s(14)), fill=ASSIGNMENT)

    destination.parent.mkdir(parents=True, exist_ok=True)
    canvas.convert("RGB").save(destination)
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()

    registry = json.loads(CONTROLS.read_text())
    illustrated = [p for p in registry["profiles"] if p.get("image")]
    digests = {}
    for profile in illustrated:
        path = render(profile, arguments.output / f"{profile['id']}.png")
        digests[profile["id"]] = hashlib.sha256(path.read_bytes()).hexdigest()[:16]
        print(f"{profile['id']:<14}{len(profile['controls']):>3} controls  {digests[profile['id']]}")

    (arguments.output / "digests.json").write_text(json.dumps(digests, indent=2, sort_keys=True) + "\n")
    print(f"\n{len(illustrated)} illustrated profiles -> {arguments.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
