"""Draw the application's geometric cartridge icon without external artwork or fonts."""

from pathlib import Path

from PIL import Image, ImageDraw


def make_icon(path: Path) -> None:
    image = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    draw.rectangle((32, 32, 992, 992), fill="#ffffff", outline="#111c40", width=32)
    draw.rectangle((244, 196, 780, 810), fill="#1647d8")
    draw.rectangle((314, 260, 710, 550), fill="#ffe52b")
    draw.rectangle((344, 790, 680, 835), fill="#303743")
    for x in range(382, 663, 56):
        draw.rectangle((x, 720, x + 18, 806), fill="#171b22")
    draw.rectangle((365, 327, 477, 439), fill="#c51b2b")
    draw.rectangle((547, 380, 659, 492), fill="#1647d8")
    image.save(path, format="ICNS" if path.suffix == ".icns" else "PNG")
