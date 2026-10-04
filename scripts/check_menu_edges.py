"""Read a photographed menu and require four edges on every highlight outline.

In the bridge we measure boxes, which does not show that the fill of a row
covered the list's right border, or that the next control covered the
bottom edge of the focused one. So we read the picture menu_shots wrote.

A highlight bar without an opposite edge, or a frame with a side that is not
in the highlight colour, is the defect: either we did not paint the outline,
or a neighbour covers it.
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
HIGHLIGHT = (0xFF, 0xF1, 0x3D)
TOLERANCE = 20

# Pictures of an open list, and of a focused control with its neighbours drawn.
# The L1 shots are the focused control with no list. The others have both.
PICTURES = [
    "native-gba-up.png",
    "native-gba-a.png",
    "disc-gba-up.png",
    "disc-gba-a.png",
    "native-ps1-l1.png",
    "disc-ps1-l1.png",
    "native-ps1-rstick.png",
    "disc-ps1-rstick.png",
    "native-retropad.png",
    "disc-retropad.png",
]


def highlight(pixel) -> bool:
    return all(abs(channel - expected) <= TOLERANCE for channel, expected in zip(pixel, HIGHLIGHT))


def horizontal_bars(image: Image.Image) -> list[tuple[int, int, int]]:
    """One entry per highlight stroke: (y of the first row, x0, x1)."""
    width, height = image.size
    pixels = image.load()
    rows: list[tuple[int, int, int]] = []
    for y in range(height):
        x = 0
        while x < width:
            if highlight(pixels[x, y]):
                start = x
                while x < width and highlight(pixels[x, y]):
                    x += 1
                if x - start >= 80:
                    rows.append((y, start, x - 1))
            else:
                x += 1
    bars: list[tuple[int, int, int]] = []
    for y, x0, x1 in rows:
        # The stroke is four pixels tall, and two outlines can share a row
        # (the callout and the list beside it). Match this run to the stroke
        # it continues, not merely to the last run we saw.
        continued = False
        for index in range(len(bars) - 1, -1, -1):
            by, bx0, bx1 = bars[index]
            if y - by > 6:
                break
            if abs(x0 - bx0) <= 2 and abs(x1 - bx1) <= 2:
                continued = True
                break
        if not continued:
            bars.append((y, x0, x1))
    return bars


def vertical_strokes(image: Image.Image) -> list[tuple[int, int, int]]:
    """Left x and the y span of each vertical highlight stroke."""
    width, height = image.size
    pixels = image.load()
    runs: list[tuple[int, int, int]] = []
    for x in range(width):
        y = 0
        while y < height:
            if highlight(pixels[x, y]):
                start = y
                while y < height and highlight(pixels[x, y]):
                    y += 1
                if y - start >= 40:
                    runs.append((x, start, y - 1))
            else:
                y += 1
    strokes: list[tuple[int, int, int]] = []
    for x, y0, y1 in runs:
        continued = False
        for index in range(len(strokes) - 1, -1, -1):
            sx, sy0, sy1 = strokes[index]
            if x - sx > 6:
                break
            if abs(y0 - sy0) <= 2 and abs(y1 - sy1) <= 2:
                continued = True
                break
        if not continued:
            strokes.append((x, y0, y1))
    return strokes


def segments(bar: tuple[int, int, int], strokes: list[tuple[int, int, int]]) -> list[tuple[int, int, int]]:
    """Split a horizontal stroke where another outline's vertical edge crosses it.

    When a callout and the list beside it touch, they form one yellow run.
    The vertical edge between them separates the two outlines.
    """
    y, x0, x1 = bar
    cuts = sorted(
        x for x, y0, y1 in strokes
        if x0 + 8 < x < x1 - 8 and y0 <= y + 2 and y1 > y + 24
    )
    pieces = []
    start = x0
    for cut in cuts:
        pieces.append((y, start, cut + 3))
        start = cut
    pieces.append((y, start, x1))
    return [piece for piece in pieces if piece[2] - piece[1] >= 80]


def covers(top: tuple[int, int, int], bottom: tuple[int, int, int]) -> bool:
    _, ax0, ax1 = top
    _, bx0, bx1 = bottom
    overlap = min(ax1, bx1) - max(ax0, bx0) + 1
    span = max(ax1 - ax0, bx1 - bx0) + 1
    return overlap >= int(span * 0.9)


def side_painted(image: Image.Image, x: int, y0: int, y1: int) -> bool:
    """True when a vertical highlight stroke runs the side, not just the corners."""
    pixels = image.load()
    width, _height = image.size
    painted = 0
    total = 0
    for y in range(y0, y1 + 1):
        total += 1
        for dx in range(4):
            column = min(max(x + dx, 0), width - 1)
            if highlight(pixels[column, y]):
                painted += 1
                break
    return total > 0 and painted / total >= 0.85


def check_image(path: Path) -> list[str]:
    image = Image.open(path).convert("RGB")
    strokes = vertical_strokes(image)
    bars = [piece for bar in horizontal_bars(image) for piece in segments(bar, strokes)]
    if not bars:
        return [f"{path.name}: no highlight outline"]
    problems: list[str] = []
    used: set[int] = set()
    frames = 0
    for index, bar in enumerate(bars):
        if index in used:
            continue
        partner = None
        for other in range(index + 1, len(bars)):
            if other in used:
                continue
            gap = bars[other][0] - bar[0]
            if gap < 24:
                continue
            if gap > 520:
                break
            if covers(bar, bars[other]):
                partner = other
                break
        y, x0, x1 = bar
        # No side starts on a bottom edge, but one does on a top with no bottom,
        # which is a focused control whose lower edge a neighbour covered. A
        # short fragment is the part of a callout still visible beside its list.
        if partner is None:
            starts_side = any(
                abs(x - x0) <= 4 and abs(y0 - y) <= 4 and y1 - y0 >= 40
                for x, y0, y1 in strokes
            )
            if starts_side and x1 - x0 >= 160:
                problems.append(
                    f"{path.name}: highlight bar y={y} x={x0}-{x1} has no opposite edge"
                )
            continue
        used.add(index)
        used.add(partner)
        frames += 1
        bottom_y = bars[partner][0]
        if not side_painted(image, x0, y, bottom_y):
            problems.append(
                f"{path.name}: left edge of the outline at x={x0} y={y}-{bottom_y} is not painted"
            )
        if not side_painted(image, x1 - 3, y, bottom_y):
            problems.append(
                f"{path.name}: right edge of the outline at x={x1} y={y}-{bottom_y} is not painted"
            )
    if frames == 0 and not problems:
        problems.append(f"{path.name}: no highlight outline")
    return problems


def main() -> int:
    directory = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "scripts/fixtures/menu-edges"
    names = sys.argv[2:] or PICTURES
    problems: list[str] = []
    for name in names:
        path = directory / name
        if not path.is_file():
            problems.append(f"{name}: picture is missing")
            continue
        problems.extend(check_image(path))
    if problems:
        print("\n".join(problems))
        print(f"{len(problems)} edge check(s) failed")
        return 1
    print(f"four edges painted in {len(names)} picture(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
