"""Pictures of selected menu screens, drawn offscreen, and
what the pictures themselves must show.

    python3 scripts/menu_pictures.py          # draw into work/feedback-pictures and check

Badges. A downloading badge must appear as an animated placeholder, and a
badge we could not fetch as a mark. A row can have the right classes and
still show a blank square, so only a picture shows the difference. We draw
every design's achievements list with a downloaded badge, one downloading
and one failed, at two moments of the animation. The middle of each waiting
badge's box must not be one flat colour, and the two moments must differ.

Disc. We draw every Disc screen that has BACK at the two window widths a
player is likely to see, for a person to look at. In `tests/menu_layout.rs`
we check the position of BACK from element boxes, not from these pictures.

Each row is a copy of the prototype of the list, as in the player
(`live_lists.cpp`). We rename its ids, set the badge state as a class, and
append an icon image only for a badge that has arrived. This check goes no
further than that imitation. We check the rows in the player in the
account-input tests, and here we check how they appear.

The second moment is 0.1 s after the first. We update `rml-preview` once,
and one RmlUi update moves an animation forward by at most 0.1 s, so we
restart the badge animations of the design 1 s in the past, and after the
update they are 0.1 s further on. This does not show that we keep drawing
the list in the running player while it is open.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from menu_states import CLI, PREVIEW, ROOT, declared_designs, difference_box, stage  # noqa: E402

OUTPUT = ROOT / "work/feedback-pictures"
STAGING = ROOT / "work/feedback-pictures-staging"
WIDTHS = (960, 1280)
HEIGHT = 600


def bundled_shaders() -> list[str]:
    listed = subprocess.run([str(CLI), "shaders"], stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, check=True)
    return [entry["id"] for entry in json.loads(listed.stdout)["result"]["presets"]]


def staged(design: str) -> Path:
    """The menu of a game with every screen: achievements, filters and
    several discs."""
    return stage("megadrive", STAGING / design, design=design, menu={
        "includeAchievements": True,
        "shaders": {"bundled": bundled_shaders()},
        "discs": 3,
    })


def draw(document: Path, target: Path, width: int, shown: dict[str, str]) -> None:
    flags: list[str] = []
    for element, value in shown.items():
        flags += ["--set", f"{element}:{value}"]
    result = subprocess.run(
        [str(PREVIEW), str(document), str(target), str(width), str(HEIGHT), *flags],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        raise SystemExit(f"could not draw {target.name}: {result.stderr.strip()[:300]}")


def showing(panel: str) -> dict[str, str]:
    """Show one screen in place of Pause, as when switching panels in the player."""
    return {"pause-panel": "display=none", panel: "display=block"}


# ---- badges ---------------------------------------------------------------

BADGE_CLASS = {"loading": "badge-loading", "failed": "badge-failed", "ready": "", "none": ""}
PROTOTYPE = re.compile(
    r'<div class="list-prototype"[^>]*>(?P<row>.*?)</div>(?=<div id="(?P<screen>[a-z-]+)-pager")',
    re.S,
)


def live_row(template: str, screen: str, row_id: str, title: str, badge: str, icon: str) -> str:
    """One row, cloned from the prototype as in the list in the player."""
    row = template.strip().replace(f'id="{screen}-prototype', f'id="{row_id}')
    if BADGE_CLASS[badge]:
        row = row.replace('class="list-row ', f'class="list-row {BADGE_CLASS[badge]} ', 1)
    row = re.sub(rf'(id="{row_id}-title"[^>]*>)(</)', rf"\g<1>{title}\2", row, count=1)
    if badge == "ready":
        closing = row.rindex("</")
        row = row[:closing] + f'<img class="list-row-icon" src="{icon}"/>' + row[closing:]
    return row


def with_rows(document: str, rows: list[tuple[str, str, str]], icon: str) -> str:
    found = PROTOTYPE.search(document)
    if not found:
        raise SystemExit("the achievements list has no row prototype")
    screen = found.group("screen")
    page = "".join(
        live_row(found.group("row"), screen, row_id, title, badge, icon)
        for row_id, title, badge in rows
    )
    return (document[: found.end()] + f'<div id="{screen}-page-1" class="list-page">{page}</div>'
            + document[found.end():])


def later(document: str, stylesheet: str) -> str:
    """The same document with every badge animation restarted 1 s ago."""
    without_keyframes = re.sub(r"@keyframes[^{]*\{(?:[^{}]*\{[^{}]*\})*[^{}]*\}", "", stylesheet)
    rules = []
    for selector, body in re.findall(r"([^{}]+)\{([^{}]*)\}", without_keyframes):
        if "badge-" not in selector:
            continue
        for value in re.findall(r"animation:\s*([^;]+)", body):
            shifted = re.sub(r"^(\s*[\d.]+m?s)", r"\1 -1s", value, count=1)
            rules.append(f"{selector.strip()} {{ animation: {shifted}; }}")
    return document.replace("</head>", f"<style>{' '.join(rules)}</style></head>", 1)


def blank(picture: Path, box) -> bool:
    """Whether the middle half of the box is one flat colour."""
    from PIL import Image

    left, top, right, bottom = box
    inset_x, inset_y = (right - left) // 4, (bottom - top) // 4
    middle = Image.open(picture).convert("RGB").crop(
        (left + inset_x, top + inset_y, right - inset_x, bottom - inset_y))
    return len(middle.getcolors(middle.size[0] * middle.size[1] or 1)) <= 1


def crop(picture: Path, box) -> bytes:
    from PIL import Image

    return Image.open(picture).convert("RGB").crop(box).tobytes()


def badges(design: str, staging: Path) -> list[str]:
    """Draw the list with waiting badges, and say what is wrong with them."""
    from PIL import Image

    Image.new("RGB", (64, 64), (200, 120, 40)).save(staging / "badge-ready.png")
    document = (staging / "menu.rml").read_text()
    stylesheet = (staging / "menu.rcss").read_text()
    shown = {**showing("achievements-panel"), "achievements-catalog": "display=block",
             "achievements-signed-out": "display=none", "heading": "text=ACHIEVEMENTS"}
    rows = [("achievement-1", "DOWNLOADED", "ready"), ("achievement-2", "DOWNLOADING", "loading"),
            ("achievement-3", "NOT FETCHED", "failed")]

    def drawn(name: str, markup: str, folder: Path) -> Path:
        source = staging / f"badges-{name}.rml"
        source.write_text(markup)
        target = folder / f"badges-{design}-{name}.png"
        draw(source, target, WIDTHS[0], shown)
        return target

    listed = with_rows(document, rows, "badge-ready.png")
    first = drawn("moment-1", listed, OUTPUT)
    second = drawn("moment-2", later(listed, stylesheet), OUTPUT)
    # A list with one waiting row differs from a list with no badge state
    # only in the area of that badge.
    plain = drawn("plain", with_rows(document, [
        (row_id, title, "ready" if badge == "ready" else "none") for row_id, title, badge in rows
    ], "badge-ready.png"), staging)
    problems = []
    for row_id, _, badge in rows[1:]:
        alone = drawn(f"only-{badge}", with_rows(document, [
            (r, t, b if r == row_id or b == "ready" else "none") for r, t, b in rows
        ], "badge-ready.png"), staging)
        box = difference_box(plain, alone)
        if box is None:
            problems.append(f"{design}: a {badge} badge draws nothing at all")
            continue
        if blank(alone, box):
            problems.append(f"{design}: a {badge} badge is a blank square: "
                            f"the middle of its box {box} is one flat colour")
        if badge == "loading" and crop(first, box) == crop(second, box):
            problems.append(f"{design}: a loading badge looks the same 0.1 s later in {box}")
    return problems


# ---- Disc -----------------------------------------------------------------

def headings(design: str) -> dict[str, str]:
    """Each screen's heading, from the design and then Native, as composed."""
    found: dict[str, str] = {}
    for package in (design, "native"):
        declared = json.loads((ROOT / "integrations/designs" / package / "design.json").read_text())
        for screen in declared.get("screens", []):
            if "heading" in screen:
                found.setdefault(screen["id"], screen["heading"])
    return found


def disc_screens(staging: Path) -> None:
    document = (staging / "menu.rml").read_text()
    titles = headings("disc")
    for screen in sorted(set(re.findall(r'id="([a-z]+)-back"', document))):
        panel = f"{screen}-panel"
        if f'id="{panel}"' not in document:
            continue
        for width in WIDTHS:
            shown = {**showing(panel), "heading": f"text={titles.get(screen, screen.upper())}"}
            draw(staging / "menu.rml", OUTPUT / f"disc-{screen}-{width}.png", width, shown)


def main() -> int:
    if not PREVIEW.exists():
        raise SystemExit(f"the offscreen preview helper is not built at {PREVIEW}")
    OUTPUT.mkdir(parents=True, exist_ok=True)
    problems: list[str] = []
    for design in declared_designs():
        staging = staged(design)
        problems += badges(design, staging)
        if design == "disc":
            disc_screens(staging)
    for problem in problems:
        print(f"  FAILED  {problem}", file=sys.stderr)
    print(f"pictures -> {OUTPUT.relative_to(ROOT)}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
