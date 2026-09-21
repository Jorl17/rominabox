"""Render each illustrated controller profile as the player draws it.

This is a tool for visual regression. We read the same declarations as the
exporter and reproduce the RmlUi overlay geometry exactly, so a change that
moves a button anchor, a callout or an illustration changes the image.

We copied this geometry from the shipped code.
  - the scene is 960x380 dp and the illustration fills it
    (integrations/designs/native/menu.rcss #controller-scene / #controller-image)
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
FONT = ROOT / "integrations/designs/native/Silkscreen-Regular.ttf"

DESIGN = ROOT / "integrations/designs/native/design.json"


def metrics() -> dict:
    """Return the scene's geometry from its design.

    We read these values from the design's stylesheet, so a change to the
    frame appears in the render that a person reviews.
    """
    return json.loads(DESIGN.read_text())["metrics"]


_METRICS = metrics()
SCENE = (_METRICS["scene"]["width"], _METRICS["scene"]["height"])
MARKER_RADIUS = _METRICS["marker"]["diameter"] // 2
CALLOUT = (_METRICS["callout"]["width"], _METRICS["callout"]["height"])
GROUP = _METRICS["group"]
SCALE = 2
MARKER = (255, 255, 255, 255)
STAGE = (32, 36, 44, 255)

DESIGNS = ROOT / "desktop/designs.json"
BASELINE = ROOT / "scripts/fixtures/overlay-digests.json"


def _rgba(value: str) -> tuple[int, int, int, int]:
    value = value.lstrip("#")
    return (int(value[0:2], 16), int(value[2:4], 16), int(value[4:6], 16), 255)


def palette(name: str | None) -> dict:
    """Return the declared colours, so that a render matches an export.

    We read the colours from `designs.json`, so a render has the palette the
    author chose, and a change to the palette appears in the picture.
    """
    declared = json.loads(DESIGNS.read_text())["palettes"]
    if name is None:
        return declared[0]
    for entry in declared:
        if entry["id"] == name:
            return entry
    available = ", ".join(entry["id"] for entry in declared)
    raise SystemExit(f"unknown palette '{name}'; declared: {available}")


def _font(size: int) -> ImageFont.ImageFont:
    if FONT.exists():
        try:
            return ImageFont.truetype(str(FONT), size)
        except OSError:
            pass
    return ImageFont.load_default()


def render(profile: dict, destination: Path, colours: dict) -> Path:
    """Draw one profile's overlay over its illustration."""
    image = ARTWORK / profile["image"]
    if not image.exists():
        raise FileNotFoundError(f"{profile['id']} declares {profile['image']}, which is missing")

    leader = _rgba(colours["muted"])
    callout_fill = _rgba(colours["surface"])
    callout_edge = _rgba(colours["edge"])
    assignment = _rgba(colours["muted"])

    canvas = Image.new("RGBA", (SCENE[0] * SCALE, SCENE[1] * SCALE), STAGE)
    canvas.alpha_composite(Image.open(image).convert("RGBA"))
    draw = ImageDraw.Draw(canvas)

    def s(value: float) -> int:
        return round(value * SCALE)

    # A group is one object on the pad. We draw one marker for its members,
    # at the anchored member, and list them together in a strip under the
    # illustration instead of giving each a place in a gutter. Seven 54 dp
    # callouts fill each gutter, so four directions per stick would cover
    # the callouts of other buttons.
    groups: dict[str, list[dict]] = {}
    for control in profile["controls"]:
        if control.get("group"):
            groups.setdefault(control["group"], []).append(control)
    ungrouped = [c for c in profile["controls"] if not c.get("group")]

    for control in ungrouped:
        x, y = control["x"], control["y"]
        callout_x, callout_y = control["calloutX"], control["calloutY"]
        # The callout's inner edge is its right side in the left gutter and its
        # left side in the right gutter.
        edge = callout_x + 200 if callout_x < 400 else callout_x
        draw.rectangle(
            [s(min(edge, x)), s(callout_y + 28) - 1, s(max(edge, x)), s(callout_y + 28) + 1],
            fill=leader,
        )
        draw.rectangle(
            [s(x) - 1, s(min(callout_y + 28, y)), s(x) + 1, s(max(callout_y + 28, y))],
            fill=leader,
        )
        draw.ellipse(
            [s(x - MARKER_RADIUS), s(y - MARKER_RADIUS),
             s(x + MARKER_RADIUS), s(y + MARKER_RADIUS)],
            outline=MARKER, width=3)

    for control in ungrouped:
        callout_x, callout_y = control["calloutX"], control["calloutY"]
        draw.rectangle(
            [s(callout_x), s(callout_y),
             s(callout_x + CALLOUT[0]), s(callout_y + CALLOUT[1])],
            fill=callout_fill,
            outline=callout_edge,
            width=3,
        )
        draw.text((s(callout_x + 10), s(callout_y + 5)), control["label"], font=_font(s(18)), fill=MARKER)
        draw.text((s(callout_x + 10), s(callout_y + 30)), control["key"], font=_font(s(14)), fill=assignment)

    # One ring on the drawn stick, and one entry in the strip with its directions.
    if groups:
        strip_w, strip_h, gap = GROUP["width"], GROUP["height"], GROUP["gap"]
        total = len(groups) * strip_w + (len(groups) - 1) * gap
        left = (SCENE[0] - total) // 2
        top = SCENE[1] - strip_h - GROUP["bottomMargin"]
        for index, (name, members) in enumerate(sorted(groups.items())):
            anchor = next((m for m in members if m["x"] or m["y"]), None)
            box_x = left + index * (strip_w + gap)
            if anchor:
                ax, ay = anchor["x"], anchor["y"]
                draw.ellipse(
                    [s(ax - MARKER_RADIUS), s(ay - MARKER_RADIUS),
                     s(ax + MARKER_RADIUS), s(ay + MARKER_RADIUS)],
                    outline=MARKER,
                    width=3,
                )
                draw.rectangle(
                    [s(ax) - 1, s(ay), s(ax) + 1, s(top)],
                    fill=leader,
                )
                draw.rectangle(
                    [s(min(ax, box_x + strip_w // 2)), s(top) - 1,
                     s(max(ax, box_x + strip_w // 2)), s(top) + 1],
                    fill=leader,
                )
            draw.rectangle(
                [s(box_x), s(top), s(box_x + strip_w), s(top + strip_h)],
                fill=callout_fill,
                outline=callout_edge,
                width=3,
            )
            # The stick's name, then its four directions on one line. On focus
            # and hover we open a fuller list, with one binding per line.
            title = name.replace("_", " ").upper()
            directions = " ".join(
                m["key"].upper()
                for m in members
                if m["id"].endswith(("_plus", "_minus"))
            )
            draw.text((s(box_x + 12), s(top + 8)), title, font=_font(s(18)), fill=MARKER)
            draw.text((s(box_x + 12), s(top + 34)), directions, font=_font(s(14)), fill=assignment)

    destination.parent.mkdir(parents=True, exist_ok=True)
    canvas.convert("RGB").save(destination)
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--record",
        action="store_true",
        help="rewrite the committed digests after a deliberate layout change",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare against the committed digests and fail on any difference",
    )
    parser.add_argument(
        "--palette",
        help="which declared palette to render; default is the first in designs.json",
    )
    arguments = parser.parse_args()

    colours = palette(arguments.palette)
    registry = json.loads(CONTROLS.read_text())
    illustrated = [p for p in registry["profiles"] if p.get("image")]
    digests = {}
    for profile in illustrated:
        path = render(profile, arguments.output / f"{profile['id']}.png", colours)
        digests[profile["id"]] = hashlib.sha256(path.read_bytes()).hexdigest()[:16]
        print(f"{profile['id']:<14}{len(profile['controls']):>3} controls  {digests[profile['id']]}")

    rendered = json.dumps(digests, indent=2, sort_keys=True) + "\n"
    (arguments.output / "digests.json").write_text(rendered)

    # Without a comparison we only render pictures to inspect by eye. With
    # the committed digests we can test that nothing moved.
    if arguments.record:
        BASELINE.parent.mkdir(parents=True, exist_ok=True)
        BASELINE.write_text(rendered)
        print(f"\nrecorded {len(digests)} digests -> {BASELINE}")
        return 0
    if arguments.check:
        if arguments.palette:
            raise SystemExit("--check compares the default palette; drop --palette")
        if not BASELINE.exists():
            raise SystemExit(f"no committed digests at {BASELINE}")
        expected = json.loads(BASELINE.read_text())
        moved = {
            name: (expected.get(name), digest)
            for name, digest in digests.items()
            if expected.get(name) != digest
        }
        missing = sorted(set(expected) - set(digests))
        if moved or missing:
            for name, (was, now) in sorted(moved.items()):
                print(f"CHANGED {name}: {was} -> {now}", file=sys.stderr)
            for name in missing:
                print(f"MISSING {name}: no longer rendered", file=sys.stderr)
            print(
                "\nA callout or anchor moved. If that was intended, look at the "
                "renders first, then re-record with --record.",
                file=sys.stderr,
            )
            return 1
        print(f"\n{len(illustrated)} profiles unchanged against {BASELINE.name}")
        return 0

    print(f"\n{len(illustrated)} illustrated profiles -> {arguments.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
