"""Render every declared state of the in-game menu, offscreen, as pictures.

From a passing test we know that a class is set, but not whether the picker
is legible, whether an open list covers the callout arrows, or whether a
selected option is clearly selected. Only a picture shows these, and
without this script we would have to launch a game to see one.

    python3 scripts/menu_states.py work/menu-states

The states are in `scripts/fixtures/menu-states.json`, where we declare them
once. We draw every design in `desktop/designs.json` for the console we
staged, in every palette. A state that is the same in two designs is an
error. A state is a small stylesheet that we append to the stylesheet of
the design, never an edit to the markup. If we matched markup strings, we
would have to change the generator and this tool together whenever an
element changed, and we use console packages to avoid that.

Each state lists the selectors it depends on, and we refuse to render a
state when any of its selectors are missing from the document. So after
someone renames an element, we report an error here instead of making a
screenshot of the wrong thing.

This does not test that the classes are set through the bridge at the
right moment, which we test with `scripts/test.py bridge`. Here we test only
how the classes appear once they are set.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ARTWORK = ROOT / "desktop/assets/controllers"
STATES = ROOT / "scripts/fixtures/menu-states.json"
# We build it here and check that it comes from this checkout, because every
# worktree shares one cargo target, so the binary next to the manifest may be
# out of date or from another checkout. See scripts/built.py.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from built import cli as _cli  # noqa: E402
import native_build  # noqa: E402
from core_source import host_target  # noqa: E402

CLI = _cli()
# The helper we package in the builder, as built in this checkout.
PREVIEW = native_build.preview_resource(host_target())

# The window size of an exported game, because at any other size a rendered
# state is not what a player sees.
SIZE = (960, 600)

# The area that the open picker covers on each console. We record it instead
# of asserting zero, because on PlayStation the stick strips are in the area
# where the list opens, so it is not zero. With a recorded figure, we see the
# defect and notice if it grows, which we would not with a tolerance.
COVERAGE = ROOT / "scripts/fixtures/picker-coverage.json"


def declared_states() -> dict[str, dict]:
    return json.loads(STATES.read_text())["states"]


def declared_designs() -> list[str]:
    """Every design we can export a game with, in registry order.

    The designs come from the same file as the palettes, so we render a new
    design there with no change here. That is the reason for more than one
    design, because we write the check once for all of them.
    """
    declared = json.loads((ROOT / "desktop/designs.json").read_text())
    return [entry["id"] for entry in declared["designs"]]


def design_dir(design: str) -> Path:
    return ROOT / "integrations/designs" / design


def mapped(items, function):
    """Run independent renders together. Each one is a separate process, and
    the pictures do not share a document or an output file."""
    if len(items) <= 1:
        return [function(item) for item in items]
    workers = min(4, len(items), os.cpu_count() or 4)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        return list(pool.map(function, items))


def stage(system: str, workspace: Path, variant: str | None = None,
          palette: str = "blue", design: str | None = None,
          menu: dict | None = None) -> Path:
    """Stage the design and the generated scene for a console, as in an export.

    Source and destination are separate directories because in the exporter we
    copy artwork from one to the other, and with both in one directory we
    would copy each file onto itself, which truncates it.

    For the placement check and the variant sweep we pass no design and use the
    first one in the registry, because those checks are about the console and
    their recorded figures do not include a design.
    """
    design = design or declared_designs()[0]
    package = design_dir(design)
    if not package.is_dir():
        raise SystemExit(f"design '{design}' is declared but {package} is not a directory")
    staged = workspace / "staged"
    staged.mkdir(parents=True, exist_ok=True)
    # Resolve from the package tree, where Native and the selected design are
    # siblings. We compose them in the shared engine, and in this harness we
    # must not make a detached package or copy a second version of the resolver.

    # We take the stylesheet from the exporter, not from the file of the design.
    # In a design we write design(surface) where a colour goes and fill it in
    # in the exporter, so a copy of the design file would render tokens and not
    # colours, and the result would differ from the shipped game.
    themed = subprocess.run(
        [str(CLI), "stage-theme"],
        input=json.dumps(
            {"source": str(package), "destination": str(staged), "palette": palette}
        ),
        capture_output=True,
        text=True,
    )
    if themed.returncode != 0:
        detail = themed.stderr.strip() or themed.stdout.strip()
        raise SystemExit(f"could not stage the {palette} palette: {detail}")
    # We generate it with the exporter, through its CLI, so that these pictures
    # show the markup we ship in an exported game and not an imitation of it.
    generated = subprocess.run(
        [str(CLI), "stage-controls"],
        input=json.dumps(
            {
                "system": system,
                "source": str(ARTWORK),
                "design": str(package),
                "destination": str(staged),
                # With stage-controls we compose the whole menu, so it
                # requires the palette, as stage-theme does.
                "palette": palette,
                # The pad an author or a player picked, staged as in the
                # exporter and not by editing its markup.
                "controls": {"profile": variant} if variant else {},
                # The other content in the game: achievements, shaders, discs.
                **(menu or {}),
            }
        ),
        capture_output=True,
        text=True,
    )
    if generated.returncode != 0:
        # A failure from the CLI is a JSON line on stdout. stderr is often empty,
        # and with an empty message we would not know why the tests failed.
        detail = generated.stderr.strip() or generated.stdout.strip()
        raise SystemExit(f"could not stage controls for {system}: {detail}")
    return staged


# `controls-device-list>1` means "the second option in the list", whatever
# its name on that console, so we can use a state on every console. With
# `controls-device-option-megadrive6`, we could render the state for one only.
ORDINAL = re.compile(r"^(?P<parent>[a-z0-9-]+)>(?P<index>\d+)$")
OPTION_ID = re.compile(r'id="(?P<id>[^"]+)"')


def options_in(document: str, parent: str) -> list[str]:
    """The element ids inside a container, in document order."""
    opening = f'id="{parent}"'
    start = document.find(opening)
    if start < 0:
        return []
    # We write the list of the picker as one flat run of buttons, so the ids
    # between this element and its closing tag are its options.
    body = document[start:]
    end = body.find("</div>")
    return [match.group("id") for match in OPTION_ID.finditer(body[:end])][1:]


def resolve(document: str, name: str) -> str | None:
    """The element listed in a state, or None when it is not in the document."""
    ordinal = ORDINAL.match(name)
    if ordinal:
        found = options_in(document, ordinal.group("parent"))
        index = int(ordinal.group("index"))
        return found[index] if index < len(found) else None
    return name if f'id="{name}"' in document else None


def consoles_offering_a_picker() -> list[str]:
    """Every console for which the player has a controller picker."""
    registry = json.loads((ROOT / "desktop/controls.json").read_text())
    counted: dict[str, int] = {}
    for profile in registry["profiles"]:
        for system in profile.get("systems", []):
            counted[system] = counted.get(system, 0) + 1
    return sorted(system for system, count in counted.items() if count > 1)


def set_flags(found: str, properties: dict) -> list[str]:
    """The renderer's --set flags for the overrides of one element. For a list
    value we set each of its values, as for a control being captured, which
    has both focused and capturing."""
    flags: list[str] = []
    for prop, value in properties.items():
        for each in value if isinstance(value, list) else [value]:
            flags += ["--set", f"{found}:{prop}={each}"]
    return flags


def render(staging: Path, target: Path, screen: str, overrides: dict[str, dict]) -> None:
    """Render `screen` as in the player, with these element overrides
    applied."""
    document = (staging / "menu.rml").read_text()
    flags: list[str] = []
    for element, properties in overrides.items():
        found = resolve(document, element)
        if found is None:
            raise SystemExit(f"{staging}: nothing named {element} in the document")
        flags += set_flags(found, properties)
    result = subprocess.run(
        [str(PREVIEW), str(staging / "menu.rml"), str(target),
         str(SIZE[0]), str(SIZE[1]), "--screen", screen, *flags],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        raise SystemExit(f"could not render {target.name}: {result.stderr.strip()[:200]}")


def difference_box(before: Path, after: Path):
    """The rectangle two renders differ in, or None when they are identical.

    We compute it instead of declaring it, because a hand-written rectangle for
    the expected place of the picker is a second copy of its position, which
    is out of date as soon as someone moves the picker in the design.
    """
    from PIL import Image, ImageChops

    a = Image.open(before).convert("RGB")
    b = Image.open(after).convert("RGB")
    return ImageChops.difference(a, b).convert("L").point(
        lambda value: 255 if value > 16 else 0
    ).getbbox()


def covered_ink(closed: Path, box) -> int:
    """How many drawn pixels an open list covers.

    We take the most common colour on the screen as the background, so any
    other colour inside the rectangle of the list is something the player
    could read before the list opened and cannot read now.
    """
    from PIL import Image

    image = Image.open(closed).convert("RGB")
    background = max(image.getcolors(image.size[0] * image.size[1]))[1]
    crop = image.crop(box)
    return sum(count for count, colour in crop.getcolors(crop.size[0] * crop.size[1] or 1)
               if colour != background)


def fixed_place(output: Path, record: bool = False) -> int:
    """Check that the picker is in the same place whichever console is loaded.

    We must not choose its rectangle by looking at the screen of one console.
    A place can be free on a Mega Drive and taken by a control on a
    PlayStation, and when the picker moves between consoles, the player has
    to look for it.

    We measure it by difference. We render the same screen with the picker
    hidden and with it shown, and the area where the two differ is the picker.
    Its expected place is not recorded here, so it cannot go out of date.

    We also render the list open and report what it covers, because a control
    in one place is only half of the solution when its list opens over the
    callouts of another console.

    This does not show that the place is a good one, only that it is one
    place, and what is hidden when the list opens.
    """
    consoles = consoles_offering_a_picker()
    if len(consoles) < 2:
        raise SystemExit(
            "only one console offers a controller picker, so this proves "
            "nothing. Check desktop/controls.json."
        )

    output.mkdir(parents=True, exist_ok=True)
    def measure(console: str):
        workspace = ROOT / f"work/menu-states-fixed/{console}"
        shutil.rmtree(workspace, ignore_errors=True)
        staging = stage(console, workspace)
        # We check against the screen at its busiest. CANCEL appears only while a
        # button is being captured, and it is in the action row, so without it
        # we could not see the picker over it.
        controls = declared_states()["controls"]
        shown = dict(
            controls["set"],
            **{"controls-cancel": {"display": "block"}},
        )
        hidden = dict(shown, **{"controls-device": {"display": "none"}})
        opened = dict(shown, **{"controls-device-list": {"display": "block"}})

        without = output / f"{console}-without.png"
        with_it = output / f"{console}-closed.png"
        with_list = output / f"{console}-open.png"
        render(staging, without, controls["screen"], hidden)
        render(staging, with_it, controls["screen"], shown)
        render(staging, with_list, controls["screen"], opened)

        closed = difference_box(without, with_it)
        opened_box = difference_box(with_it, with_list)
        if closed is None:
            # When nothing is drawn, the consoles do not agree. Three missing boxes
            # are not a match, so we report a picker moved over another control.
            return console, f"{console}: hiding the picker changed nothing, so it drew nothing", None, None, None
        if opened_box is None:
            return console, f"{console}: opening the list changed nothing", None, None, None
        # We check both rectangles against the screen without the picker. The
        # closed control must be in a free band, and so must its list.
        cover = max(covered_ink(without, closed), covered_ink(without, opened_box))
        return console, None, closed, opened_box, cover

    closed_boxes: dict[str, tuple] = {}
    open_boxes: dict[str, tuple] = {}
    covers: dict[str, int] = {}
    failures: list[str] = []
    for console, failure, closed, opened_box, cover in mapped(consoles, measure):
        if failure:
            failures.append(failure)
            continue
        closed_boxes[console] = closed
        open_boxes[console] = opened_box
        covers[console] = cover
        print(f"  {console:<12}closed {closed}  open {opened_box}  covers {cover} drawn px")

    if failures:
        for failure in failures:
            print(f"  FAILED  {failure}", file=sys.stderr)
        return 1

    places = set(closed_boxes.values())
    if len(places) > 1:
        print(
            f"\nthe picker is in {len(places)} different places depending on the "
            "console. It is one control and it belongs in one place; a player "
            "who learns where it is on one game should not have to find it "
            "again on the next.",
            file=sys.stderr,
        )
        for console, box in sorted(closed_boxes.items()):
            print(f"  {console}: {box}", file=sys.stderr)
        return 1

    if record:
        COVERAGE.write_text(json.dumps(covers, indent=2, sort_keys=True) + "\n")
        print(f"\nrecorded picker coverage -> {COVERAGE.name}")
        return 0

    recorded = json.loads(COVERAGE.read_text()) if COVERAGE.exists() else {}
    worse = {
        console: (recorded.get(console, 0), count)
        for console, count in covers.items()
        if count > recorded.get(console, 0)
    }
    if worse:
        for console, (was, now) in sorted(worse.items()):
            print(f"  WORSE   {console}: covers {now} px, was {was}", file=sys.stderr)
        print(
            "\nOpening the picker now hides more of the screen than it did. The "
            "closed control is in a free band; its list opens into the scene, "
            "and what it lands on is something the player was reading.\n"
            "If this is deliberate, look at the pictures and re-record:\n"
            "  python3 scripts/menu_states.py --fixed-place --record work/menu-states-fixed",
            file=sys.stderr,
        )
        return 1

    unsolved = {c: n for c, n in covers.items() if n > 0}
    print(
        f"\nthe picker is in the same place on all {len(consoles)} consoles that "
        "have one"
    )
    if unsolved:
        print(
            "  still covered when open: "
            + ", ".join(f"{c} {n} px" for c, n in sorted(unsolved.items()))
            + "  (unsolved: the stick strips sit where the list opens)"
        )
    return 0


def every_variant(output: Path) -> int:
    """Check that every pad a player can pick has a picture.

    In the export we stage the drawing of every pad, not only the chosen one,
    so that choosing the six-button Mega Drive shows a six-button pad as well
    as changing the emulated device and the label. In this test we render pads
    other than the one the author exported.

    We stage each offered pad in turn, as in the exporter, and report an error
    for any pad that we render without its artwork.

    The change of picture in the player is tested in the bridge tests.
    """
    registry = json.loads((ROOT / "desktop/controls.json").read_text())
    jobs = [
        (console, entry["id"])
        for console in consoles_offering_a_picker()
        for entry in registry["profiles"]
        if console in entry.get("systems", [])
    ]

    def draw(job: tuple[str, str]):
        console, variant = job
        problems: list[str] = []
        workspace = ROOT / f"work/menu-variants/{console}-{variant}"
        shutil.rmtree(workspace, ignore_errors=True)
        staging = stage(console, workspace, variant)
        document = (staging / "menu.rml").read_text()
        state = declared_states()["controls"]
        overrides: list[str] = []
        for element, properties in state["set"].items():
            target = resolve(document, element)
            if target is None:
                problems.append(f"{console}/{variant}: {element} is absent")
                continue
            for prop, value in properties.items():
                overrides += ["--set", f"{target}:{prop}={value}"]
        output.mkdir(parents=True, exist_ok=True)
        target_png = output / f"{console}-{variant}.png"
        result = subprocess.run(
            [str(PREVIEW), str(staging / "menu.rml"), str(target_png),
             str(SIZE[0]), str(SIZE[1]), "--screen", state["screen"], *overrides],
            capture_output=True, text=True,
        )
        if result.returncode != 0:
            problems.append(f"{console}/{variant}: {result.stderr.strip()[:120]}")
            return problems, ""
        if "Could not load texture" in result.stderr:
            problems.append(f"{console}/{variant}: its artwork is not staged")
            return problems, ""
        # A scene file for each pad, because we cannot generate markup in the
        # player and read a scene file when someone changes the controller.
        beside = staging / f"scene-{variant}.rml"
        if not beside.exists():
            problems.append(f"{console}/{variant}: no {beside.name} to swap to")
            return problems, ""
        return problems, f"  {console:<12}{variant:<14}{beside.name}\n"

    failures: list[str] = []
    rendered = 0
    for problems, line in mapped(jobs, draw):
        failures.extend(problems)
        if line:
            print(line, end="")
            rendered += 1

    if failures:
        for failure in failures:
            print(f"  FAILED  {failure}", file=sys.stderr)
        print(
            f"\n{len(failures)} pad(s) can be picked but cannot be drawn. A "
            "picker offering a controller the game cannot show is a picker that "
            "changes the label and nothing else.",
            file=sys.stderr,
        )
        return 1
    print(f"\nall {rendered} offered pads draw, and each has a scene to swap to")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    # There is no default, because it depends on the console we draw, and two
    # consoles in one directory collide. We run the test scopes in parallel,
    # and if one of them emptied the directory, we would delete pictures still
    # in use by another.
    parser.add_argument("output", type=Path, nargs="?", default=None)
    parser.add_argument("--system", default="megadrive", help="which console to stage")
    parser.add_argument(
        "--variant",
        help="which of the console's controllers to draw (default: the console's own)",
    )
    parser.add_argument(
        "--record",
        action="store_true",
        help="with --fixed-place, record where the picker lands, after looking at it",
    )
    parser.add_argument(
        "--every-variant",
        action="store_true",
        help="check every pad a player can pick has artwork and a scene to swap to",
    )
    parser.add_argument(
        "--fixed-place",
        action="store_true",
        help="check the picker lands in the same place on every console that has one",
    )
    arguments = parser.parse_args()

    if not PREVIEW.exists():
        raise SystemExit(f"the offscreen preview helper is not built at {PREVIEW}")

    if arguments.output is None:
        arguments.output = ROOT / f"work/menu-states/{arguments.system}"

    if arguments.every_variant:
        return every_variant(arguments.output)

    if arguments.fixed_place:
        return fixed_place(arguments.output, arguments.record)

    # We empty the directory, not only create it, because the old picture of a
    # renamed state would stay and show something that no longer exists as if
    # it were current.
    shutil.rmtree(arguments.output, ignore_errors=True)
    arguments.output.mkdir(parents=True, exist_ok=True)

    missing: list[str] = []
    digests: dict[str, str] = {}
    rendered = 0
    # We draw every state of every design in every palette. The key contains the
    # design and the console, so we record a second design next to the first,
    # and two consoles never share a directory.
    for design in declared_designs():
        print(f"\n{design}")
        for palette in palettes():
            # Design and console are both part of the path. We run the test
            # scopes in parallel, and with two consoles or two designs staged
            # in one directory, the files of one run would replace those of
            # the other, and we would see a state that changed.
            workspace = (
                ROOT / "work" / "menu-states-staging" / design / arguments.system / palette
            )
            shutil.rmtree(workspace, ignore_errors=True)
            staging = stage(
                arguments.system, workspace, arguments.variant, palette, design
            )
            document = (staging / "menu.rml").read_text()
            print(f"  {palette}")
            rendered += render_palette(
                design, arguments.system, palette, staging, document,
                arguments, digests, missing,
            )

    return finish(arguments, digests, missing, rendered)


def palettes() -> list[str]:
    """Every colour scheme we can export a game with.

    We declare three palettes, and we can see a CSS rule with a colour from
    the palette, or a design with a hardcoded colour, only when we render
    every palette.

    The palettes come from the same file as in the exporter, so we render and
    pin a new palette there with no change here. There is one check for all
    palettes, not one per palette.
    """
    declared = json.loads((ROOT / "desktop/designs.json").read_text())
    return [entry["id"] for entry in declared["palettes"]]


def draw_state(design: str, system: str, palette: str, name: str, state: dict, staging: Path, document: str, output: Path):
    """One picture. Returns (key, digest or None, line for stdout, line for stderr)."""
    key = f"{design}/{system}/{palette}/{name}"
    # In a state we can list what the console must offer for the state to make
    # sense. The picker states require a picker, and a console with one
    # controller has none, so refusing them there would mark every such console
    # as failed for a screen it rightly lacks. A renamed element still gives an
    # error, because the other elements of the state are listed and missing.
    needs = state.get("needs")
    if needs and resolve(document, needs) is None:
        return key, None, f"  {name:<26}not offered by this console\n", ""

    # An id ending in "?" is optional in the state. A console whose pad has
    # sticks has a box for each stick, and a console without sticks has none.
    # Both are correct, so we do not refuse the state on a console without
    # sticks.
    wanted = {i.rstrip("?"): (v, i.endswith("?")) for i, v in state["set"].items()}
    resolved = {i: resolve(document, i) for i in wanted}
    absent = [i for i, found in resolved.items() if found is None and not wanted[i][1]]
    if absent:
        # We do not skip it, because when the element of a state is gone,
        # someone has renamed or removed that element.
        return key, None, "", f"  REFUSED {key}: {', '.join(absent)} not in the document\n"

    overrides: list[str] = []
    for element_id, (properties, _) in wanted.items():
        if resolved[element_id] is None:
            continue
        overrides += set_flags(resolved[element_id], properties)

    target = output / f"{name}.png"
    result = subprocess.run(
        [
            str(PREVIEW),
            str(staging / "menu.rml"),
            str(target),
            str(SIZE[0]),
            str(SIZE[1]),
            "--screen",
            state["screen"],
            *overrides,
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return key, None, "", f"  FAILED  {key}: {result.stderr.strip()[:160]}\n"
    # We report a texture that does not load instead of raising, because the
    # scene is then rendered without its controller and only looks empty, for
    # example when a PNG is truncated.
    if "Could not load texture" in result.stderr:
        return key, None, "", f"  FAILED  {key}: artwork did not load\n"
    digest = hashlib.sha256(target.read_bytes()).hexdigest()[:16]
    return key, digest, f"  {name:<26}{state['describes']}\n", ""


def render_palette(design, system, palette, staging, document, arguments, digests, missing) -> int:
    """Draw every declared state in one design, one console and one colour scheme."""
    # The output root is already per console, so two test scopes never empty the
    # same directory. Design and palette still have separate directories under
    # it, because two designs with pause-menu.png in one folder also collide.
    output = arguments.output / design / palette
    output.mkdir(parents=True, exist_ok=True)
    states = list(declared_states().items())
    drawn = mapped(
        states,
        lambda item: draw_state(
            design, system, palette, item[0], item[1], staging, document, output
        ),
    )
    rendered = 0
    for key, digest, line, error in drawn:
        if line:
            print(line, end="")
        if error:
            print(error, end="", file=sys.stderr)
            missing.append(key)
            continue
        # For a state that this console does not offer we drew nothing, so there is
        # nothing to pin. If we recorded it, every skipped state would look the
        # same as every other, and the twin check would report them.
        if digest is None:
            continue
        digests[key] = digest
        rendered += 1
    return rendered


def same_picture(digests: dict[str, str]) -> tuple[list[list[str]], list[str]]:
    """Pictures that should differ and do not.

    We ask two separate questions, because the causes differ. When two states
    in one design, console and palette give the same picture, the player
    cannot tell hover from keyboard focus or a held button. When two designs
    give the same picture for one state, the second design is not a design.
    """
    within: dict[tuple[str, str, str, str], list[str]] = {}
    across: dict[tuple[str, str, str, str], list[str]] = {}
    for key, digest in digests.items():
        design, system, palette, state = key.split("/", 3)
        within.setdefault((design, system, palette, digest), []).append(key)
        across.setdefault((system, palette, state, digest), []).append(design)
    twins = [names for names in within.values() if len(names) > 1]
    collided = []
    for (system, palette, state, _digest), designs in sorted(across.items()):
        unique = sorted(set(designs))
        if len(unique) > 1:
            collided.append(
                f"{system}/{palette}/{state}: {', '.join(unique)} draw the same picture"
            )
    return twins, collided


def finish(arguments, digests, missing, rendered) -> int:
    """Report twins, refusals and digest drift across every design drawn."""

    twins, collided = same_picture(digests)
    for names in twins:
        print(
            f"  IDENTICAL {', '.join(sorted(names))}: the same picture",
            file=sys.stderr,
        )
    for line in collided:
        print(f"  SAME DESIGN {line}", file=sys.stderr)

    if missing:
        print(
            f"\n{len(missing)} declared state(s) did not render: {', '.join(missing)}.\n"
            "Either an element was renamed, its artwork is missing, or this "
            f"console does not offer it — '{arguments.system}' was staged.",
            file=sys.stderr,
        )
        return 1

    if twins:
        print(
            f"\n{len(twins)} group(s) of states render identically. Either the "
            "design draws no difference between them — which is the defect, "
            "because the player cannot see one either — or they are the same "
            "state declared twice.",
            file=sys.stderr,
        )
        return 1

    if len(declared_designs()) < 2:
        print(
            "\nonly one design is declared, so nothing compared two designs. "
            "A state that renders once proves nothing about the second one.",
            file=sys.stderr,
        )
        return 1

    if collided:
        print(
            f"\n{len(collided)} state(s) render identically across designs. "
            "The same palette and the same state must not come out the same "
            "picture: a second design that draws the first one has not changed "
            "what a menu is.",
            file=sys.stderr,
        )
        return 1

    print(f"\n{rendered} states -> {arguments.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
