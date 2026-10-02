"""Render the builder's menu preview for every design in the builder.

The preview is the only picture in the builder that we draw with the menu
renderer of the player. A build can pass every other test and still show "The
preview could not be rendered." for every design, for example with this error:

    this design has no <!--BINDS--> for the binds on a control.

    python3 scripts/test_menu_preview.py

We render here and do not compare pictures. We check the appearance of a menu
in the states tests. Here we check that we can draw in the builder the design
that the author picked at all, and that the author's background picture
appears behind the menu and nowhere else, and not over the running game,
where we draw the overlays with the same document.

We also check that the preview matches the player at the same size, with
the menu canvas scaled to fit the picture as we scale it to the window in the
player, and with a `text` change of an element drawn as the markup written
by the exporter, as we set it in the picture tests.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KIT = ROOT / "desktop/src-tauri/resources/runtime"
DESIGNS = ROOT / "integrations/designs"
ARTWORK = ROOT / "desktop/assets/controllers"

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
import scratch  # noqa: E402
from built import cli as _cli  # noqa: E402
from core_source import host_target  # noqa: E402

RENDERER = native_build.preview_resource(host_target())


def palettes() -> list[str]:
    declared = json.loads((ROOT / "desktop/designs.json").read_text())
    return [entry["id"] for entry in declared["palettes"]]


def render(design: str, palette: str, into: Path, background: Path | None = None,
           source: bool = False, size: tuple[int, int] = (960, 600), tint: bool = False) -> str:
    """Return an empty string when the drawing worked, otherwise the reason.

    We draw from the kit, as the builder does, because a design missing from
    the kit is the failure we check for here. With `source` we compose from
    the repository's designs and shared parts instead, to check a change to
    them that a kit staged before the change does not show."""
    request = {
        "design": str((DESIGNS if source else KIT / "designs") / design),
        "assets": str(ARTWORK if source else KIT / "menu-assets"),
        "renderer": str(RENDERER),
        "outputDir": str(into),
        "palette": palette,
        "width": size[0],
        "height": size[1],
    }
    if background is not None:
        request["background"] = str(background)
    if tint:
        request["tintBackground"] = True
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


def draw(document: Path, picture: Path, *changes: str) -> str:
    """Return an empty string when we drew a composed document at 960 by 600
    with these `--set` changes in the renderer, otherwise the reason."""
    flags = [flag for change in changes for flag in ("--set", change)]
    completed = subprocess.run(
        [str(RENDERER), str(document), str(picture), "960", "600", *flags],
        capture_output=True, text=True, timeout=180,
    )
    if completed.returncode != 0 or not picture.is_file():
        return completed.stderr.strip() or f"{picture} was not written"
    return ""


# A colour that no palette uses, so every pixel in it is from the author's picture.
PICTURE = (255, 0, 255)


def picture_share(image: Path, colour: tuple[int, int, int] = PICTURE, within: int = 0) -> float:
    """Return the share of `image` in `colour`, with each channel within `within`."""
    from PIL import Image, ImageChops

    picture = Image.open(image).convert("RGB")
    masks = [channel.point(lambda value, wanted=wanted: 255 if abs(value - wanted) <= within else 0)
             for channel, wanted in zip(picture.split(), colour)]
    near = ImageChops.multiply(ImageChops.multiply(masks[0], masks[1]), masks[2])
    return near.histogram()[255] / (picture.width * picture.height)


# When the author of a menu chooses it, we multiply the author's picture by
# the screen colour of the palette (integrations/parts/background.rcss), and
# round each channel of that product in the renderer.
TINT_ROUNDING = 2


def tinted_picture(palette: str) -> tuple[int, int, int]:
    """Return the author's picture, every pixel PICTURE, as we draw it tinted
    in the menu of `palette`."""
    declared = json.loads((ROOT / "desktop/designs.json").read_text())
    screen = next(entry["screen"] for entry in declared["palettes"] if entry["id"] == palette)
    channels = [int(screen[index:index + 2], 16) for index in (1, 3, 5)]
    return tuple(round(made * tint / 255) for made, tint in zip(PICTURE, channels))


def background_picture(area: Path) -> Path:
    """Return the author's picture, with every pixel in the colour that no palette uses."""
    from PIL import Image

    picture = area / "background-source.png"
    if not picture.is_file():
        Image.new("RGB", (64, 40), PICTURE).save(picture)
    return picture


def overlay_problem(design: str, area: Path) -> str:
    """Return an empty string when none of a game's background picture appears
    while the game runs. In the player we draw the same document over the
    game, with `overlay` on the body, for the splash, the notice and an
    unlock, and the background belongs only to the menu, never to the game."""
    into = area / f"{design}-overlay"
    problem = render(design, palettes()[0], into, background_picture(area), source=True)
    if problem:
        return problem
    menu = (into / "menu.rml").read_text()
    body = '<body id="body">'
    if menu.count(body) != 1:
        return f"the composed menu has no single {body} to mark overlay"
    running = into / "overlay.rml"
    running.write_text(menu.replace(body, '<body id="body" class="overlay">'))
    drawn = into / "overlay.png"
    problem = draw(running, drawn)
    if problem:
        return f"the overlay did not draw: {problem}"
    share = picture_share(drawn)
    if share:
        return f"{share:.1%} of the running game is covered by the menu's background picture"
    return ""


def background_problem(design: str, area: Path) -> str:
    """Return an empty string when a game's background picture appears behind
    this design's menu as made, or in the screen's colour when the author
    chose a tint, and none appears for a game without one."""
    picture = background_picture(area)
    tinted = tinted_picture(palettes()[0])
    shares = {}
    for label, background, tint in (("with", picture, False), ("tinted", picture, True),
                                    ("without", None, False)):
        into = area / f"{design}-background-{label}"
        problem = render(design, palettes()[0], into, background, tint=tint)
        if problem:
            return f"{label}: {problem}"
        drawn = into / "preview.png"
        shares[label] = (picture_share(drawn), picture_share(drawn, tinted, TINT_ROUNDING))
    if shares["without"] != (0, 0):
        return f"the picture shows in a game without one ({max(shares['without']):.1%})"
    if shares["with"][0] < 0.05:
        return f"only {shares['with'][0]:.1%} of the menu shows the background picture as the author made it"
    if shares["tinted"][0] or shares["tinted"][1] < 0.05:
        return (f"asked for tinted, {shares['tinted'][1]:.1%} of the menu shows the picture tinted "
                f"and {shares['tinted'][0]:.1%} as the author made it")
    return ""


# How far a menu drawn twice as large and then halved may differ from the menu
# drawn at its own size, as the average difference per colour channel, out of
# 255. We draw the letters again at each size, so the two are never identical,
# and the difference is far larger when we draw the canvas unscaled.
SCALE_TOLERANCE = 5.0


def scale_problem(design: str, area: Path) -> str:
    """Return an empty string when we scale the menu to the picture in the
    preview as in the player. Drawn twice as large and halved, it is then the
    menu at its own size, not a menu of that size in a larger picture."""
    from PIL import Image, ImageChops, ImageStat

    pictures = []
    for width, height in ((960, 600), (1920, 1200)):
        into = area / f"{design}-{width}x{height}"
        problem = render(design, palettes()[0], into, size=(width, height))
        if problem:
            return f"at {width}x{height}: {problem}"
        pictures.append(Image.open(into / "preview.png").convert("RGB"))
    small, large = pictures
    halved = large.resize(small.size, Image.Resampling.BOX)
    difference = sum(ImageStat.Stat(ImageChops.difference(small, halved)).mean) / 3
    if difference > SCALE_TOLERANCE:
        return (f"drawn at 1920x1200 and halved, the menu differs from the menu drawn at "
                f"960x600 by {difference:.2f} a channel (at most {SCALE_TOLERANCE})")
    return ""


def markup_problem(design: str, area: Path) -> str:
    """Return an empty string when we draw a `text` change as markup. In the
    picture tests we set the text of an element to the markup from the exporter
    (the label of an options button is a span), and the tags must not appear."""
    into = area / f"{design}-markup"
    problem = render(design, palettes()[0], into)
    if problem:
        return problem
    drawn = into / "markup.png"
    colour = "#{:02x}{:02x}{:02x}".format(*PICTURE)
    problem = draw(into / "menu.rml", drawn,
                   f'heading:text=<span style="color: {colour};">MARKUP</span>')
    if problem:
        return problem
    if not picture_share(drawn):
        return f"a heading set to a span coloured {colour} shows none of that colour"
    return ""


def paging_problem(design: str, area: Path) -> str:
    """Return an empty string when we split a list into pages in the preview as
    in the player, from the first page and with the pager showing. We give the
    composed Options list a page size of one, so CONTROLS fills the first page
    and HOTKEYS is on the second. We paint each in the colour that no palette
    uses in its own picture, and the pager too. Without pages, HOTKEYS appears
    in the preview, and in the Disc design its last entries ran over the
    volume row."""
    into = area / f"{design}-paging"
    problem = render(design, palettes()[0], into, source=True)
    if problem:
        return problem
    menu = (into / "menu.rml").read_text(encoding="utf-8")
    sized = re.subn(r'(id="options-list" class="list" data-page-size=")\d+"', r'\g<1>1"', menu)
    if sized[1] != 1:
        return "the composed menu has no single Options list with a page size"
    document = into / "paging.rml"
    document.write_text(sized[0], encoding="utf-8")
    colour = "#{:02x}{:02x}{:02x}".format(*PICTURE)
    shown = ["--set", "pause-panel:display=none", "--set", "options-panel:display=block"]
    seen = {}
    for element in ("controls", "hotkeys", "options-pager"):
        drawn = into / f"paging-{element}.png"
        completed = subprocess.run(
            [str(RENDERER), str(document), str(drawn), "960", "600", *shown,
             "--set", f"{element}:background-color={colour}"],
            capture_output=True, text=True, timeout=180,
        )
        if completed.returncode != 0 or not drawn.is_file():
            return completed.stderr.strip() or f"{drawn} was not written"
        seen[element] = picture_share(drawn) > 0
    if not seen["controls"]:
        return "CONTROLS, on the first page, does not show"
    if seen["hotkeys"]:
        return "HOTKEYS, on the second page, shows: the list is drawn unsplit"
    if not seen["options-pager"]:
        return "the pager of a list of several pages does not show"
    return ""


def screen_problem(design: str, area: Path) -> str:
    """Return an empty string when `--screen` shows a screen as the player
    does, with only its panel and its own footer. We draw Options with the
    Pause CONTINUE in the colour no palette uses, then its own CONTROLS, and
    Pause and Options with the footer in it. Options must hide Pause and show
    its own entries, and different footer words must give different footers.
    A picture of Options once kept the Pause footer."""
    into = area / f"{design}-screen"
    problem = render(design, palettes()[0], into, source=True)
    if problem:
        return problem
    colour = "#{:02x}{:02x}{:02x}".format(*PICTURE)
    shares = {}
    for screen, element, prop in (("options", "resume", "background-color"),
                                  ("options", "controls", "background-color"),
                                  ("pause", "footer-hint", "color"),
                                  ("options", "footer-hint", "color")):
        drawn = into / f"screen-{screen}-{element}.png"
        completed = subprocess.run(
            [str(RENDERER), str(into / "menu.rml"), str(drawn), "960", "600",
             "--screen", screen, "--set", f"{element}:{prop}={colour}"],
            capture_output=True, text=True, timeout=180,
        )
        if completed.returncode != 0 or not drawn.is_file():
            return completed.stderr.strip() or f"{drawn} was not written"
        shares[(screen, element)] = picture_share(drawn)
    if shares[("options", "resume")]:
        return "Options shown, Pause's CONTINUE still shows"
    if not shares[("options", "controls")]:
        return "Options shown, its CONTROLS does not show"
    footers = declared_footers(into / "design.cfg")
    if footers.get("pause") != footers.get("options") \
            and shares[("pause", "footer-hint")] == shares[("options", "footer-hint")]:
        return (f"Options' footer ({footers.get('options')}) draws as Pause's "
                f"({footers.get('pause')})")
    return ""


def settings_problem(design: str, area: Path) -> str:
    """Return an empty string when we draw the player settings in the preview
    as in a game before we have a value from RetroArch: on Options, the volume
    slider filled to its high end and the state of PLAY IN BACKGROUND in words,
    each in the colour that no palette uses in its own picture. Drawn as
    composed, they show an empty fill and no state."""
    into = area / f"{design}-settings"
    problem = render(design, palettes()[0], into, source=True)
    if problem:
        return problem
    colour = "#{:02x}{:02x}{:02x}".format(*PICTURE)
    for element, prop, what in (("volume-level-fill", "background-color", "the volume's fill"),
                                ("background-play-state", "color", "PLAY IN BACKGROUND's state")):
        drawn = into / f"settings-{element}.png"
        completed = subprocess.run(
            [str(RENDERER), str(into / "menu.rml"), str(drawn), "960", "600",
             "--screen", "options", "--set", f"{element}:{prop}={colour}"],
            capture_output=True, text=True, timeout=180,
        )
        if completed.returncode != 0 or not drawn.is_file():
            return completed.stderr.strip() or f"{drawn} was not written"
        if not picture_share(drawn):
            return f"{what} is not drawn"
    return ""


def declared_footers(design_cfg: Path) -> dict[str, str]:
    """Return the footer of each screen, as declared in the composed design.cfg
    that we read in the player."""
    return {found.group(1): found.group(2) for found in
            re.finditer(r'^screen_footer_([a-z-]+) = "(.*)"$',
                        design_cfg.read_text(encoding="utf-8"), re.MULTILINE)}


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
            for name, check in (("background", background_problem),
                                ("over the game", overlay_problem),
                                ("at twice the size", scale_problem),
                                ("text as markup", markup_problem),
                                ("a list in pages", paging_problem),
                                ("a screen shown", screen_problem),
                                ("settings drawn", settings_problem)):
                problem = check(design, area)
                if problem:
                    print(f"  FAIL {design} {name}: {problem}", file=sys.stderr)
                    failures.append(f"{design} {name}")
                else:
                    print(f"  ok   {design} {name}")

    if failures:
        print(
            f"\n{len(failures)} preview check(s) failed: {', '.join(failures)}.\n"
            "The author sees 'The preview could not be rendered.' for a design "
            "and palette that did not draw.",
            file=sys.stderr,
        )
        return 1
    print(f"\nthe builder draws all {len(designs)} designs in every palette")
    return 0


if __name__ == "__main__":
    sys.exit(main())
