"""Render the controller artwork from its SVG source.

We ship every pad as an SVG and a PNG side by side. In the player we need the
PNG, because RmlUi in our fork is compiled without SVG support, and in the
builder we prefer the SVG. So the drawing exists twice, and the two copies can
differ.

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
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ARTWORK = ROOT / "desktop/assets/controllers"
DESIGN = ROOT / "integrations/designs/native/design.json"
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


def render(svg: Path, destination: Path, width: int, height: int) -> None:
    subprocess.run(
        [renderer(), "-w", str(width), "-h", str(height), str(svg), "-o", str(destination)],
        check=True,
    )


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()[:16]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
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

    scratch = ROOT / "work/controller-render"
    scratch.mkdir(parents=True, exist_ok=True)
    drifted: list[str] = []
    digests: dict[str, str] = {}

    for svg in sources:
        png = svg.with_suffix(".png")
        target = (scratch / png.name) if arguments.check else png
        render(svg, target, width * SCALE, height * SCALE)
        digests[svg.stem] = digest(target)

        if arguments.check:
            if not png.exists():
                print(f"  MISSING {png.name}", file=sys.stderr)
                drifted.append(svg.stem)
                continue
            if digest(png) != digests[svg.stem]:
                print(f"  DRIFTED {png.name}", file=sys.stderr)
                drifted.append(svg.stem)
            else:
                print(f"  ok      {png.name}")
        else:
            print(f"  wrote   {png.name}")

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
