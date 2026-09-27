"""Render each illustrated controller profile as the player draws it.

This is a tool for visual regression. We read the same declarations as the
exporter, and we get the place of every callout, ring, leader and stick box
from the exporter (`rominabox-cli scene-geometry`, computed in
`scene_layout.rs`), so a change that moves a button anchor, a callout, a route
or an illustration changes the image. We compute no placement here, and the
size of the scene comes from the design.

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
            # Use the basic layout on every machine. In Pillow, raqm is the
            # layout when the raqm library loads, and the basic one otherwise
            # (Windows, without FriBiDi), and letters can differ by a pixel.
            return ImageFont.truetype(str(FONT), size, layout_engine=ImageFont.Layout.BASIC)
        except OSError:
            pass
    return ImageFont.load_default()


# We build it here and check that it comes from this checkout, because every
# worktree shares one cargo target, so the binary next to the manifest may be
# out of date or from another checkout. See scripts/built.py.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from built import cli as _cli  # noqa: E402

CLI = _cli()


def scene_geometry(profile_id: str) -> dict:
    """Ask the exporter where everything on the scene goes.

    We do not compute it here. A person reviews these renders before a
    release, so they must match the exported game, and we can be sure of
    that only when we take the positions from the exporter.
    """
    import subprocess

    if not CLI.exists():
        raise SystemExit(
            f"{CLI.name} is not built, and it owns the scene's geometry:\n"
            "  cargo build --release --manifest-path "
            "desktop/src-tauri/Cargo.toml --bin rominabox-cli"
        )
    system = next(
        (s for p in json.loads(CONTROLS.read_text())["profiles"]
         if p["id"] == profile_id for s in p.get("systems", [])),
        None,
    )
    if system is None:
        raise SystemExit(f"no console offers the {profile_id} pad, so it has no scene")
    asked = subprocess.run(
        [str(CLI), "scene-geometry"],
        input=json.dumps(
            {"system": system, "profile": profile_id, "design": str(DESIGN.parent)}
        ),
        capture_output=True,
        text=True,
    )
    if asked.returncode != 0:
        raise SystemExit(f"could not ask for {profile_id}'s geometry: {asked.stderr}")
    return json.loads(asked.stdout)["result"]


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

    def horizontal(at_y: float, x0: float, x1: float) -> None:
        draw.rectangle([s(min(x0, x1)), s(at_y) - 1, s(max(x0, x1)), s(at_y) + 1], fill=leader)

    def vertical(at_x: float, y0: float, y1: float) -> None:
        draw.rectangle([s(at_x) - 1, s(min(y0, y1)), s(at_x) + 1, s(max(y0, y1))], fill=leader)

    def route(runs: list[dict]) -> None:
        for run in runs:
            if run["height"] == 0:
                horizontal(run["y"], run["x"], run["x"] + run["width"])
            else:
                vertical(run["x"], run["y"], run["y"] + run["height"])

    def ring(marker: dict) -> None:
        draw.ellipse(
            [s(marker["x"]), s(marker["y"]),
             s(marker["x"] + marker["width"]), s(marker["y"] + marker["height"])],
            outline=MARKER, width=3)

    def box(rect: dict, title: str, detail: str) -> None:
        draw.rectangle(
            [s(rect["x"]), s(rect["y"]), s(rect["x"] + rect["width"]), s(rect["y"] + rect["height"])],
            fill=callout_fill,
            outline=callout_edge,
            width=3,
        )
        draw.text((s(rect["x"] + 10), s(rect["y"] + 5)), title, font=_font(s(18)), fill=MARKER)
        draw.text((s(rect["x"] + 10), s(rect["y"] + 30)), detail, font=_font(s(14)), fill=assignment)

    # We take the position of every callout, ring, leader and stick box from
    # the exporter, which we use to make the shipped game, and the text of a
    # callout's bindings from the game. We compute none of the positions
    # here.
    geometry = scene_geometry(profile["id"])
    declared = {control["id"]: control for control in profile["controls"]}
    for placement in geometry["controls"]:
        route(placement["leader"])
        ring(placement["marker"])
    for placement in geometry["controls"]:
        control = declared[placement["id"]]
        box(placement["callout"], control["label"], "")

    # A stick is one object on the pad, with one ring at the member placed on
    # the pad and one box with the binding of every direction.
    for group in geometry["groups"]:
        route(group["leader"])
        if group["marker"]:
            ring(group["marker"])
        box(group["strip"], geometry["titles"][group["name"]], "")

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
    (arguments.output / "digests.json").write_text(rendered, encoding="utf-8", newline="\n")

    # Without a comparison we only render pictures to inspect by eye. With
    # the committed digests we can test that nothing moved.
    if arguments.record:
        BASELINE.parent.mkdir(parents=True, exist_ok=True)
        BASELINE.write_text(rendered, encoding="utf-8", newline="\n")
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
