"""Require a lit ring to stand out from the pad all the way round.

We draw the ring over a control only while the pointer is on the control or
the control has focus. On a pale drawing, such as GameCube's Control stick,
a white ring on a white stick is hard to see. We draw the Controls screen
offscreen with that stick plain, hovered and focused, in every design, and
take the ring from where the pictures differ. At every few degrees round its
circle, the lit picture must differ in brightness from the plain one by a
clear step somewhere across the ring's line.

    python3 scripts/check_ring_contrast.py [OUTPUT]

This does not show that the ring is in the right place (menu_scene), or how
it looks on any pad but GameCube's.
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import menu_states  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
SYSTEM = "gamecube"
STICK = "control-group-l_stick"
# A step in brightness, out of 255, that a person sees as an edge.
CLEAR = 96
# The step, in degrees, between the points we test round the ring.
STEP = 6


def brightness(pixel) -> float:
    red, green, blue = pixel[:3]
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def components(points: set[tuple[int, int]]) -> list[set[tuple[int, int]]]:
    """The groups of touching points."""
    left = set(points)
    found = []
    while left:
        seed = left.pop()
        group = {seed}
        frontier = [seed]
        while frontier:
            x, y = frontier.pop()
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    near = (x + dx, y + dy)
                    if near in left:
                        left.remove(near)
                        group.add(near)
                        frontier.append(near)
        found.append(group)
    return found


def ring(plain: Image.Image, lit: Image.Image, diameter: int) -> tuple[float, float, float] | None:
    """The centre and radius of the round group of pixels that differ
    between the two pictures, about `diameter` across."""
    width, height = plain.size
    a, b = plain.load(), lit.load()
    changed = {
        (x, y)
        for y in range(height)
        for x in range(width)
        if a[x, y] != b[x, y]
    }
    best = None
    for group in components(changed):
        xs = [x for x, _ in group]
        ys = [y for _, y in group]
        across, down = max(xs) - min(xs) + 1, max(ys) - min(ys) + 1
        if abs(across - down) > 4 or abs(across - diameter) > 10:
            continue
        centre = ((max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2)
        if best is None or len(group) > best[0]:
            best = (len(group), centre, across / 2)
    if best is None:
        return None
    _, (x, y), radius = best
    return x, y, radius


def unseen(plain: Image.Image, lit: Image.Image, circle: tuple[float, float, float]) -> list[int]:
    """The angles, in degrees, at which nothing across the ring's line
    differs from the plain picture by a clear step."""
    x, y, radius = circle
    a, b = plain.load(), lit.load()
    faint = []
    for degrees in range(0, 360, STEP):
        angle = math.radians(degrees)
        steps = []
        for out in range(int(radius) - 6, int(radius) + 2):
            px = round(x + out * math.cos(angle))
            py = round(y + out * math.sin(angle))
            steps.append(abs(brightness(b[px, py]) - brightness(a[px, py])))
        if max(steps) < CLEAR:
            faint.append(degrees)
    return faint


def check(design: str, output: Path) -> list[str]:
    workspace = output / design
    staging = menu_states.stage(SYSTEM, workspace / "staged", None, "blue", design)
    document = (staging / "menu.rml").read_text()
    shown = {"pause-panel": {"display": "none"}, "controls-panel": {"display": "block"}}
    states = {
        "plain": shown,
        "hovered": dict(shown, **{STICK: {"pseudo": "hover"}}),
        "focused": dict(shown, **{STICK: {"class": "focused"}}),
    }
    pictures = {}
    for name, overrides in states.items():
        _, _, _, error = menu_states.draw_state(
            design, SYSTEM, "blue", name, {"describes": name, "set": overrides},
            staging, document, workspace,
        )
        if error:
            return [error.strip()]
        pictures[name] = Image.open(workspace / f"{name}.png").convert("RGB")
    diameter = marker(design)
    problems = []
    for name in ("hovered", "focused"):
        circle = ring(pictures["plain"], pictures[name], diameter)
        if circle is None:
            problems.append(f"{design}: no ring was drawn round the {name} stick")
            continue
        faint = unseen(pictures["plain"], pictures[name], circle)
        if faint:
            problems.append(
                f"{design}: the {name} stick's ring cannot be told from the pad at "
                f"{len(faint)} of {360 // STEP} places round it (degrees {faint[:8]}...)"
            )
        print(f"{design} {name}: ring at {circle[0]:.0f},{circle[1]:.0f}, faint at {len(faint)} places")
    return problems


def marker(design: str) -> int:
    """The diameter of the stick's ring in `design`, as we place it on export."""
    import json
    import subprocess

    asked = subprocess.run(
        [str(menu_states.CLI), "scene-geometry"],
        input=json.dumps({"system": SYSTEM, "design": str(menu_states.design_dir(design))}),
        capture_output=True,
        text=True,
        check=True,
    )
    groups = json.loads(asked.stdout)["result"]["groups"]
    return next(group["marker"]["width"] for group in groups if f"control-group-{group['name']}" == STICK)


def main() -> int:
    output = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "work" / "ring-contrast"
    problems = [
        problem for design in menu_states.declared_designs() for problem in check(design, output)
    ]
    for problem in problems:
        print(problem, file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
