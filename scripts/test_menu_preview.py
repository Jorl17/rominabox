"""Render the builder's menu preview for every design in the builder.

The preview is the only picture in the builder that we draw with the menu
renderer of the player. A build can pass every other test and still show "The
preview could not be rendered." for every design, for example with this error:

    this design has no <!--BINDS--> for the binds on a control.

    python3 scripts/test_menu_preview.py

We render here and do not compare pictures. We check the appearance of a menu
in the states tests. Here we check that we can draw in the builder the design
that the author picked at all.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KIT = ROOT / "desktop/src-tauri/resources/runtime"
RENDERER = ROOT / "desktop/src-tauri/resources/preview/rml-preview"
DESIGNS = ROOT / "integrations/designs"

sys.path.insert(0, str(Path(__file__).resolve().parent))
import scratch  # noqa: E402
from built import cli as _cli  # noqa: E402


def palettes() -> list[str]:
    declared = json.loads((ROOT / "desktop/designs.json").read_text())
    return [entry["id"] for entry in declared["palettes"]]


def render(design: str, palette: str, into: Path, background: Path | None = None) -> str:
    """Return an empty string when the drawing worked, otherwise the reason it failed."""
    request = {
        # We read them from the kit, as in the builder, and not from
        # the source tree. A design missing from the kit is the
        # failure that we check for here.
        "design": str(KIT / "designs" / design),
        "assets": str(KIT / "menu-assets"),
        "renderer": str(RENDERER),
        "outputDir": str(into),
        "palette": palette,
        "width": 960,
        "height": 600,
    }
    if background is not None:
        request["background"] = str(background)
    asked = subprocess.run(
        [str(_cli()), "preview"],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        timeout=180,
    )
    if asked.returncode != 0:
        return asked.stderr.strip() or asked.stdout.strip()
    try:
        result = json.loads(asked.stdout)
    except json.JSONDecodeError:
        return asked.stdout.strip()
    if result.get("type") == "error":
        return result.get("message", "the renderer refused")
    image = Path(result["result"]["imagePath"])
    if not image.is_file() or image.stat().st_size == 0:
        return f"{image} was not written"
    return ""


# A colour that no palette uses, so every pixel in it is from the author's picture.
PICTURE = (255, 0, 255)


def picture_share(image: Path) -> float:
    from PIL import Image

    pixels = list(Image.open(image).convert("RGB").getdata())
    return sum(1 for pixel in pixels if pixel == PICTURE) / len(pixels)


def background_problem(design: str, area: Path) -> str:
    """Return an empty string when a game's background picture appears behind
    this design's menu, and none appears for a game without one."""
    from PIL import Image

    picture = area / "background-source.png"
    Image.new("RGB", (64, 40), PICTURE).save(picture)
    shares = {}
    for label, background in (("with", picture), ("without", None)):
        into = area / f"{design}-background-{label}"
        problem = render(design, palettes()[0], into, background)
        if problem:
            return f"{label} a background: {problem}"
        shares[label] = picture_share(into / "preview.png")
    if shares["without"] != 0:
        return f"the picture shows in a game without one ({shares['without']:.1%})"
    if shares["with"] < 0.05:
        return f"only {shares['with']:.1%} of the menu shows the background picture"
    return ""


def main() -> int:
    if not RENDERER.is_file():
        raise SystemExit(f"no offscreen renderer at {RENDERER}")
    designs = sorted(p.name for p in DESIGNS.iterdir() if p.is_dir())
    if not designs:
        raise SystemExit(f"no designs in {DESIGNS}")

    failures: list[str] = []
    with scratch.scratch() as temporary:
        area = Path(temporary)
        for design in designs:
            for palette in palettes():
                problem = render(design, palette, area / f"{design}-{palette}")
                if problem:
                    print(f"  FAIL {design}/{palette}: {problem}", file=sys.stderr)
                    failures.append(f"{design}/{palette}")
                else:
                    print(f"  ok   {design}/{palette}")
            problem = background_problem(design, area)
            if problem:
                print(f"  FAIL {design} background: {problem}", file=sys.stderr)
                failures.append(f"{design} background")
            else:
                print(f"  ok   {design} background")

    if failures:
        print(
            f"\n{len(failures)} preview(s) the builder cannot draw: "
            f"{', '.join(failures)}.\n"
            "The author sees 'The preview could not be rendered.' for these.",
            file=sys.stderr,
        )
        return 1
    print(f"\nthe builder draws all {len(designs)} designs in every palette")
    return 0


if __name__ == "__main__":
    sys.exit(main())
