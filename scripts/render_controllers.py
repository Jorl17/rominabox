"""Render the controller artwork from its SVG source.

We ship every pad as an SVG and a PNG side by side. In the player we need the
PNG, because RmlUi in our fork is compiled without SVG support, and in the
builder we prefer the SVG. So the drawing exists twice, and the two copies can
differ.

Each PNG is a uniform scale of its own SVG, placed on the scene. Nine of the
ten SVGs are 256x256 ES-DE icons and the scene is 960x380, so a render
straight into the canvas would stretch them. We scale each pad proportionally,
by its own factor between 1.31 and 1.73, and position it. `placement.json`
contains that placement, derived from the shipped artwork.

    python3 scripts/render_controllers.py            # regenerate every PNG
    python3 scripts/render_controllers.py --check    # fail if any has drifted

The SVG is the source and the PNG is output. With `--check` we catch a PNG
that someone edited directly.

Rendering requires `rsvg-convert` (librsvg). We do not vendor it, because an
exported game does not include build tools.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ARTWORK = ROOT / "desktop/assets/controllers"
DESIGN = ROOT / "integrations/designs/native/design.json"
DESIGN_DIR = DESIGN.parent
CONTROLS = ROOT / "desktop/controls.json"
# We build it here and check that it comes from this checkout, because every
# worktree shares one cargo target, so the binary next to the manifest may be
# out of date or from another checkout. See scripts/built.py.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from built import cli as _cli  # noqa: E402

CLI = _cli()
BASELINE = ROOT / "scripts/fixtures/controller-digests.json"

# We render at twice the scene size so the artwork stays crisp on a retina
# display, with the same factor as scripts/render_control_overlays.py.
SCALE = 2


def scene_size() -> tuple[int, int]:
    """Return the frame the artwork fills, from the design."""
    metrics = json.loads(DESIGN.read_text())["metrics"]["scene"]
    return metrics["width"], metrics["height"]


def renderer() -> str:
    found = shutil.which("rsvg-convert")
    if not found:
        raise SystemExit(
            "rsvg-convert is not installed (brew install librsvg).\n"
            "It renders the controller artwork; it is a build tool and is not "
            "shipped with an exported game."
        )
    return found


def placement_of(svg: Path) -> dict:
    """Read each pad's placement from the file beside its drawing.

    With one file for every controller, adding a pad would change a file that
    every other pad depends on, which can cause merge conflicts and
    accidental deletions.
    """
    beside = svg.with_suffix(".json")
    if not beside.exists():
        raise SystemExit(
            f"{svg.name} has no placement at {beside.name}. Derive it from the "
            "shipped PNG before rendering, or the drawing will land in the "
            "wrong place and every button anchor will miss."
        )
    return json.loads(beside.read_text())


def render(svg: Path, destination: Path, width: int, height: int) -> None:
    """Render proportionally and place the drawing on the scene canvas.

    A render straight into the canvas would stretch nine of the ten pads,
    because their SVGs are 256x256 ES-DE icons and the scene is 960x380. Only
    the Game Boy is drawn in the aspect of the scene. We scale each pad by
    its own factor, between 1.31 and 1.73, and centre it.

    `placement.json` contains that placement, derived from the shipped
    artwork, so a regenerated PNG is exactly where the button anchors are.
    """
    from PIL import Image  # only needed to compose, and only by this tool

    spot = placement_of(svg)
    natural = destination.with_name(f".{svg.stem}.natural.png")
    subprocess.run(
        [renderer(), "-h", str(spot["imageHeight"]), str(svg), "-o", str(natural)],
        check=True,
    )
    drawing = Image.open(natural).convert("RGBA")
    canvas = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    canvas.paste(drawing, (spot["imageX"], spot["imageY"]), drawing)
    canvas.save(destination)
    natural.unlink(missing_ok=True)


# We compare where the drawing is and whether it is the same drawing, and not
# the bytes. rsvg and the tool that made the originals differ in compression
# and in antialiasing at the edges, so with a byte comparison all ten would
# differ although every one of them is correct.
BOX_TOLERANCE = 2      # pixels; rounding a scale factor moves an edge by one
PIXEL_TOLERANCE = 3.0  # percent; antialiasing along a long outline


def compare(shipped: Path, rendered: Path) -> str:
    """Return why the rendered artwork does not match, or nothing when it does."""
    from PIL import Image

    a = Image.open(shipped).convert("RGBA")
    b = Image.open(rendered).convert("RGBA")
    if a.size != b.size:
        return f"size {b.size} against {a.size}"

    # The button anchors depend on the bounding box. When the drawing moves,
    # every callout points at the wrong place.
    box_a, box_b = a.split()[-1].getbbox(), b.split()[-1].getbbox()
    if box_a and box_b:
        drift = max(abs(x - y) for x, y in zip(box_a, box_b))
        if drift > BOX_TOLERANCE:
            return f"drawing moved by {drift}px, so the button anchors no longer fit"

    differing = sum(1 for p, q in zip(a.getdata(), b.getdata()) if p != q)
    percent = 100 * differing / (a.size[0] * a.size[1])
    if percent > PIXEL_TOLERANCE:
        return f"{percent:.1f}% of pixels differ, which is more than antialiasing"
    return ""

def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()[:16]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--stage-frontend",
        action="store_true",
        help="copy the SVG sources where the builder UI can load them",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare the shipped PNGs against a fresh render and fail on drift",
    )
    arguments = parser.parse_args()

    width, height = scene_size()
    sources = sorted(ARTWORK.glob("controller-*.svg"))
    if not sources:
        raise SystemExit(f"no controller SVGs in {ARTWORK}")

    if arguments.stage_frontend:
        # In the builder, which is a browser, we draw the SVG directly. We need
        # a PNG only in the player, because RmlUi there has no SVG support. We
        # stage the SVG instead of committing a copy, so there is still one
        # copy of each drawing under version control.
        destination = ROOT / "desktop/public/controllers"
        destination.mkdir(parents=True, exist_ok=True)
        for svg in sources:
            shutil.copyfile(svg, destination / svg.name)
            shutil.copyfile(svg.with_suffix(".json"), destination / f"{svg.stem}.json")
        # We take the positions of the rings, callouts and leader lines from
        # the exporter instead of computing them again for the builder, so the
        # builder shows the same positions as the shipped game.
        staged = 0
        for profile in json.loads(CONTROLS.read_text())["profiles"]:
            if not profile.get("image"):
                continue
            system = next(iter(profile.get("systems", [])), None)
            if system is None:
                continue
            asked = subprocess.run(
                [str(CLI), "scene-geometry"],
                input=json.dumps(
                    {"system": system, "profile": profile["id"], "design": str(DESIGN_DIR)}
                ),
                capture_output=True,
                text=True,
            )
            if asked.returncode != 0:
                raise SystemExit(
                    f"could not ask for {profile['id']}'s geometry: {asked.stderr}"
                )
            layout = json.loads(asked.stdout)["result"]
            (destination / f"{profile['image'].removesuffix('.png')}-layout.json").write_text(
                json.dumps(layout, indent=2, sort_keys=True) + "\n"
            )
            staged += 1
        print(
            f"staged {len(sources)} SVGs, their placement and {staged} layouts "
            f"-> {destination}"
        )
        return 0

    scratch = ROOT / "work/controller-render"
    scratch.mkdir(parents=True, exist_ok=True)

    def draw(svg: Path):
        png = svg.with_suffix(".png")
        target = (scratch / png.name) if arguments.check else png
        render(svg, target, width * SCALE, height * SCALE)
        verdict = ""
        if arguments.check:
            verdict = "missing" if not png.exists() else compare(png, target)
        return svg, target, verdict

    workers = min(4, len(sources), os.cpu_count() or 4)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        drawn = list(pool.map(draw, sources))

    drifted: list[str] = []
    digests: dict[str, str] = {}
    for svg, target, verdict in drawn:
        png = svg.with_suffix(".png")
        digests[svg.stem] = digest(target)
        if not arguments.check:
            print(f"  wrote   {png.name}")
            continue
        if verdict == "missing":
            print(f"  MISSING {png.name}", file=sys.stderr)
            drifted.append(svg.stem)
        elif verdict:
            print(f"  DRIFTED {png.name}: {verdict}", file=sys.stderr)
            drifted.append(svg.stem)
        else:
            print(f"  ok      {png.name}")

    if arguments.check:
        if drifted:
            print(
                f"\n{len(drifted)} PNG(s) do not match their SVG: {', '.join(drifted)}.\n"
                "The SVG is the source. Regenerate with:\n"
                "  python3 scripts/render_controllers.py",
                file=sys.stderr,
            )
            return 1
        print(f"\nall {len(sources)} PNGs match their SVG")
        return 0

    BASELINE.parent.mkdir(parents=True, exist_ok=True)
    BASELINE.write_text(json.dumps(digests, indent=2, sort_keys=True) + "\n")
    print(f"\nrendered {len(sources)} controllers at {width * SCALE}x{height * SCALE}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
