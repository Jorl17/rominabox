"""Cut the Science Gothic font in which we draw ROM-in-a-Box's name in games.

The builder's `desktop/src/fonts/ScienceGothic.ttf` has a weight axis. In the
game's menu renderer we use a font file as one fixed face, so for games we
take a fixed Bold (700, the weight of the builder's wordmark) of that same
file, limited to Latin-1: printable ASCII, the · and × in the menus, and the
accented letters of game and achievement names:

    python3 scripts/cut_game_font.py
"""

from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "desktop/src/fonts/ScienceGothic.ttf"
CUT = ROOT / "integrations/designs/native/ScienceGothic-Bold.ttf"
WEIGHT = 700


def main() -> None:
    face = instancer.instantiateVariableFont(TTFont(SOURCE), {"wght": WEIGHT})
    face["OS/2"].usWeightClass = WEIGHT
    options = subset.Options()
    options.layout_features = ["kern", "liga"]
    options.name_IDs = ["*"]
    options.notdef_outline = True
    cutter = subset.Subsetter(options)
    cutter.populate(unicodes=[*range(0x20, 0x7F), *range(0xA0, 0x100)])
    cutter.subset(face)
    face.save(CUT)
    print(f"{CUT.relative_to(ROOT)}: {CUT.stat().st_size} bytes")


if __name__ == "__main__":
    main()
