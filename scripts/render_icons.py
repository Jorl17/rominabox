"""Render every ROM-in-a-Box icon from the one drawing.

    python3 scripts/render_icons.py            # render them all
    python3 scripts/render_icons.py --check    # fail if one has drifted

The source is the splash logo, `desktop/assets/branding/logo.svg`, white line
art that we draw on the screen colour of the default palette. We read that
colour from `desktop/designs.json` and `desktop/defaults.json`. The outputs:

  desktop/src-tauri/icons/icon.icns   the builder on macOS, a full-bleed
                                      square that appears as a squircle on
                                      macOS 26 (a squircle drawn into the
                                      icon itself appears boxed in grey)
  desktop/src-tauri/icons/icon.ico    the builder on Windows, the squircle
                                      with its corners clear
  desktop/src-tauri/icons/icon.png    that squircle, for the builder's header
  desktop/src-tauri/icons/icon-large.png  a larger one, where we show it big
                                      in the builder (the drop area, a game's
                                      icon before the author picks one)
  desktop/assets/default-icon.png     a game without artwork, the full-bleed
                                      square, which we keep whole at export

At small sizes we draw the lines heavier, as icon sets do, because at 16 and
32 pixels the logo's own line weight is a hairline.

Rendering requires `rsvg-convert` and, for the `.icns`, macOS's `iconutil`. We
commit the outputs, so a build on a machine without them uses those files.
With --check we render only the compared pictures, so it works everywhere.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from render_controllers import compare, renderer  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
LOGO = ROOT / "desktop/assets/branding/logo.svg"
ICONS = ROOT / "desktop/src-tauri/icons"
DEFAULT_ICON = ROOT / "desktop/assets/default-icon.png"
MASTER = 1024
# The position of the drawing in its 256-unit viewBox, so we centre it by what
# is drawn and not by the empty margin around it.
DRAWN = (19, 55, 237, 220)
# The logo's width on the square, and the squircle's inset from the canvas
# on Windows, as fractions of the side.
LOGO_WIDTH = 0.68
WINDOWS_INSET = 0.06
# Apple's app shape is close to a superellipse of this exponent.
SQUIRCLE = 5.0
ICNS = [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)]
ICO = [16, 24, 32, 48, 64, 128, 256]
# We show it 56 pixels wide in the header. This is twice that size, for
# double density, with the lines of a 56-pixel icon.
HEADER = 112
LARGE = 256


def ground() -> tuple[int, int, int, int]:
    """The default palette's screen colour."""
    palette = json.loads((ROOT / "desktop/defaults.json").read_text())["palette"]
    palettes = json.loads((ROOT / "desktop/designs.json").read_text())["palettes"]
    colour = next(entry for entry in palettes if entry["id"] == palette)["screen"]
    return tuple(int(colour[i:i + 2], 16) for i in (1, 3, 5)) + (255,)


def weight(size: int) -> float:
    """Return how much heavier we draw the logo's lines at `size` pixels."""
    return 1.0 if size >= 128 else 1.35 if size >= 48 else 1.8


def logo(width: int, heavier: float, scratch: Path):
    from PIL import Image

    text = re.sub(
        r'stroke-width="(\d+(?:\.\d+)?)"',
        lambda found: f'stroke-width="{float(found.group(1)) * heavier:g}"',
        LOGO.read_text(encoding="utf-8"),
    )
    source = scratch / f"logo-{heavier}.svg"
    source.write_text(text, encoding="utf-8")
    drawn = scratch / f"logo-{width}-{heavier}.png"
    subprocess.run([renderer(), "-w", str(width), "-h", str(width), str(source), "-o", str(drawn)], check=True)
    return Image.open(drawn).convert("RGBA")


def square(size: int, scratch: Path, shown: int | None = None):
    """Return the full-bleed square at `size` pixels, drawn at full size and
    reduced, with the lines for the size we show it at (`size` by default)."""
    from PIL import Image

    face = Image.new("RGBA", (MASTER, MASTER), ground())
    width = round(MASTER * LOGO_WIDTH)
    art = logo(width, weight(shown or size), scratch)
    scale = width / 256
    left = MASTER / 2 - (DRAWN[0] + DRAWN[2]) / 2 * scale
    top = MASTER / 2 - (DRAWN[1] + DRAWN[3]) / 2 * scale
    face.alpha_composite(art, (round(left), round(top)))
    return face if size == MASTER else face.resize((size, size), Image.LANCZOS)


def squircle_mask(size: int, inset: float):
    """Apple's app shape in a square `size` pixels wide, `inset` of a side in."""
    from PIL import Image, ImageDraw

    scale = 4
    big = size * scale
    half = big * (1 - 2 * inset) / 2
    centre = big / 2
    points = []
    for step in range(2048):
        angle = 2 * math.pi * step / 2048
        c, s = math.cos(angle), math.sin(angle)
        points.append((
            centre + half * math.copysign(abs(c) ** (2 / SQUIRCLE), c),
            centre + half * math.copysign(abs(s) ** (2 / SQUIRCLE), s),
        ))
    mask = Image.new("L", (big, big), 0)
    ImageDraw.Draw(mask).polygon(points, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def tile(size: int, scratch: Path, shown: int | None = None):
    """The squircle with its corners clear, at `size` pixels."""
    from PIL import Image

    shaped = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shaped.paste(square(size, scratch, shown), (0, 0), squircle_mask(size, WINDOWS_INSET))
    return shaped


def render(into: Path, containers: bool = True) -> dict[str, Path]:
    """Write every output under `into`, at its path in the repository.
    Without `containers`, write all but the .icns and .ico, which we only look
    for in the check (the .icns requires macOS's iconutil)."""
    written: dict[str, Path] = {}
    with tempfile.TemporaryDirectory(prefix="rominabox-icons-") as made:
        scratch = Path(made)
        (into / ICONS.relative_to(ROOT)).mkdir(parents=True, exist_ok=True)
        if containers:
            iconset = scratch / "icon.iconset"
            iconset.mkdir()
            for size, density in ICNS:
                name = f"icon_{size}x{size}{'@2x' if density == 2 else ''}.png"
                square(size * density, scratch).save(iconset / name)
            icns = into / ICONS.relative_to(ROOT) / "icon.icns"
            subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(icns)], check=True)
            written["icns"] = icns

            frames = [tile(size, scratch) for size in ICO]
            ico = into / ICONS.relative_to(ROOT) / "icon.ico"
            frames[-1].save(ico, format="ICO", sizes=[(size, size) for size in ICO], append_images=frames[:-1])
            written["ico"] = ico

        header = into / ICONS.relative_to(ROOT) / "icon.png"
        tile(HEADER, scratch, shown=HEADER // 2).save(header)
        written["header"] = header

        large = into / ICONS.relative_to(ROOT) / "icon-large.png"
        tile(LARGE, scratch).save(large)
        written["large"] = large

        default = into / DEFAULT_ICON.relative_to(ROOT)
        default.parent.mkdir(parents=True, exist_ok=True)
        square(MASTER, scratch).save(default)
        written["default"] = default
    return written


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="fail if a committed icon differs from a fresh render")
    arguments = parser.parse_args()
    if not arguments.check:
        if not shutil.which("iconutil"):
            raise SystemExit("iconutil is macOS's; the icons are rendered on a Mac and committed")
        for name, path in render(ROOT).items():
            print(f"{name}: {path.relative_to(ROOT)}")
        return 0
    drifted = []
    with tempfile.TemporaryDirectory(prefix="rominabox-icons-check-") as made:
        fresh = render(Path(made), containers=False)
        for name in ("header", "large", "default"):
            shipped = ROOT / fresh[name].relative_to(made)
            why = compare(shipped, fresh[name]) if shipped.is_file() else "missing"
            if why:
                drifted.append(f"{shipped.relative_to(ROOT)}: {why}")
        for shipped in (ICONS / "icon.icns", ICONS / "icon.ico"):
            if not shipped.is_file():
                drifted.append(f"{shipped.relative_to(ROOT)}: missing")
    if drifted:
        print("icons that no longer match logo.svg (run python3 scripts/render_icons.py):")
        print("\n".join(f"  {line}" for line in drifted))
        return 1
    print("every icon matches logo.svg")
    return 0


if __name__ == "__main__":
    sys.exit(main())
