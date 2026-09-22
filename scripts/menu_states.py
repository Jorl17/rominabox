"""Render every declared state of the in-game menu, offscreen, as pictures.

From a passing test we know that a class is set, but not whether the picker
is legible, whether an open list covers the callout arrows, or whether a
selected option is clearly selected. Only a picture shows these, and
without this script we would have to launch a game to see one.

    python3 scripts/menu_states.py work/menu-states
    python3 scripts/menu_states.py --palette <id> work/menu-states
    python3 scripts/menu_states.py --check

The states are in `scripts/fixtures/menu-states.json` and the palettes are
in `desktop/designs.json`. In one run we draw every state in every palette,
as `<output>/<palette>/<state>.png`, or one palette with `--palette`. The
keys of the digest are the palette and then the state, because one state in
two palettes gives two pictures. We reject a digest in the old flat form,
without palettes, and with `--check` we ask for a new recording.

A state is a small stylesheet that we append to the stylesheet of the design,
never an edit to the markup. If we matched markup strings, we would have to
change the generator and this tool together whenever an element changed, and
we use console packages to avoid that kind of duplication.

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
import re
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import worktree

ROOT = Path(__file__).resolve().parent.parent
# The freshly built helper when there is one, because a staged copy is a
# build output that can be older than a change to the helper.
BUILT_PREVIEW = ROOT / "work/experiments/rml-preview/build/rml-preview"
STAGED_PREVIEW = ROOT / "desktop/src-tauri/resources/preview/rml-preview"
PREVIEW = BUILT_PREVIEW if BUILT_PREVIEW.exists() else STAGED_PREVIEW
DESIGN = ROOT / "integrations/designs/native"
ARTWORK = ROOT / "desktop/assets/controllers"
STATES = ROOT / "scripts/fixtures/menu-states.json"
DIGESTS = ROOT / "scripts/fixtures/menu-state-digests.json"
DESIGNS = ROOT / "desktop/designs.json"
CLI = worktree.cli_path(ROOT)

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


def declared_palettes() -> list[str]:
    """Palette ids from desktop/designs.json, in the order they are declared.

    We read them instead of listing them here, so we render a new palette from
    that file in the next run. The names come only from this place.
    """
    names = [entry["id"] for entry in json.loads(DESIGNS.read_text())["palettes"]]
    if not names:
        raise SystemExit(f"{DESIGNS.name} declares no palettes")
    return names


def require_palette(name: str) -> str:
    declared = declared_palettes()
    if name not in declared:
        raise SystemExit(f"unknown palette '{name}'; declared: {', '.join(declared)}")
    return name


def stage(system: str, workspace: Path, variant: str | None = None,
          palette: str | None = None) -> Path:
    """Stage the design and the generated scene for a console, as in an export.

    Source and destination are separate directories because in the exporter we
    copy artwork from one to the other, and with both in one directory we
    would copy each file onto itself, which truncates it.
    """
    # For callers without a palette, which are the fixed-place and every-variant
    # checks, we still render one palette, the first one declared.
    palette = declared_palettes()[0] if palette is None else require_palette(palette)
    source = workspace / "source"
    staged = workspace / "staged"
    for directory in (source, staged):
        directory.mkdir(parents=True, exist_ok=True)
    # The whole design package, because that is what a design is. With a
    # separate list of its documents here, we could stage something else than
    # the kit and render the tests in a different frame from the one in the kit.
    for document in DESIGN.iterdir():
        if document.is_file():
            shutil.copyfile(document, source / document.name)
            shutil.copyfile(document, staged / document.name)

    # We take the stylesheet from the exporter, not from the file of the design.
    # In a design we write design(surface) where a colour goes and fill it in
    # in the exporter, so a copy of the design file would render tokens and not
    # colours, and the result would differ from the shipped game.
    themed = subprocess.run(
        [str(CLI), "stage-theme"],
        input=json.dumps(
            {"source": str(source), "destination": str(staged), "palette": palette}
        ),
        capture_output=True,
        text=True,
    )
    if themed.returncode != 0:
        raise SystemExit(f"could not stage the {palette} palette: {themed.stderr}")
    for art in ARTWORK.glob("controller-*.png"):
        shutil.copyfile(art, source / art.name)
    shutil.copyfile(ARTWORK / "CONTROLLERS.txt", source / "CONTROLLERS.txt")

    # We generate it with the exporter, through its CLI, so that these pictures
    # show the markup we ship in an exported game and not an imitation of it.
    generated = subprocess.run(
        [str(CLI), "stage-controls"],
        input=json.dumps(
            {
                "system": system,
                "source": str(source),
                "design": str(source),
                "destination": str(staged),
                # The pad an author or a player picked, staged as in the
                # exporter and not by editing its markup.
                "controls": {"profile": variant} if variant else {},
            }
        ),
        capture_output=True,
        text=True,
    )
    if generated.returncode != 0:
        raise SystemExit(f"could not stage controls for {system}: {generated.stderr}")
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


def render(staging: Path, target: Path, overrides: dict[str, dict]) -> None:
    """Render the controls screen with these element overrides applied."""
    document = (staging / "menu.rml").read_text()
    flags: list[str] = []
    for element, properties in overrides.items():
        found = resolve(document, element)
        if found is None:
            raise SystemExit(f"{staging}: nothing named {element} in the document")
        for prop, value in properties.items():
            flags += ["--set", f"{found}:{prop}={value}"]
    result = subprocess.run(
        [str(PREVIEW), str(staging / "menu.rml"), str(target),
         str(SIZE[0]), str(SIZE[1]), *flags],
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
    closed_boxes: dict[str, tuple] = {}
    open_boxes: dict[str, tuple] = {}
    covers: dict[str, int] = {}
    failures: list[str] = []

    for console in consoles:
        workspace = ROOT / f"work/menu-states-fixed/{console}"
        shutil.rmtree(workspace, ignore_errors=True)
        staging = stage(console, workspace)
        # We check against the screen at its busiest. CANCEL appears only while a
        # button is being captured, and it is in the action row, so without it
        # we could not see the picker over it.
        shown = dict(
            declared_states()["controls"]["set"],
            **{"controls-cancel": {"display": "block"}},
        )
        hidden = dict(shown, **{"controls-device": {"display": "none"}})
        opened = dict(shown, **{"controls-device-list": {"display": "block"}})

        without = output / f"{console}-without.png"
        with_it = output / f"{console}-closed.png"
        with_list = output / f"{console}-open.png"
        render(staging, without, hidden)
        render(staging, with_it, shown)
        render(staging, with_list, opened)

        closed = difference_box(without, with_it)
        opened_box = difference_box(with_it, with_list)
        if closed is None:
            # When nothing is drawn, the consoles do not agree. Three missing boxes
            # are not a match, so we report a picker moved over another control.
            failures.append(f"{console}: hiding the picker changed nothing, so it drew nothing")
            continue
        if opened_box is None:
            failures.append(f"{console}: opening the list changed nothing")
            continue
        closed_boxes[console] = closed
        open_boxes[console] = opened_box
        # We check both rectangles against the screen without the picker. The
        # closed control must be in a free band, and so must its list.
        covers[console] = max(
            covered_ink(without, closed), covered_ink(without, opened_box)
        )
        print(f"  {console:<12}closed {closed}  open {opened_box}  covers {covers[console]} drawn px")

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
    failures: list[str] = []
    rendered = 0
    for console in consoles_offering_a_picker():
        registry = json.loads((ROOT / "desktop/controls.json").read_text())
        variants = [
            entry["id"]
            for entry in registry["profiles"]
            if console in entry.get("systems", [])
        ]
        for variant in variants:
            workspace = ROOT / f"work/menu-variants/{console}-{variant}"
            shutil.rmtree(workspace, ignore_errors=True)
            staging = stage(console, workspace, variant)
            document = (staging / "menu.rml").read_text()
            state = declared_states()["controls"]
            overrides: list[str] = []
            for element, properties in state["set"].items():
                target = resolve(document, element)
                if target is None:
                    failures.append(f"{console}/{variant}: {element} is absent")
                    continue
                for prop, value in properties.items():
                    overrides += ["--set", f"{target}:{prop}={value}"]
            output.mkdir(parents=True, exist_ok=True)
            target_png = output / f"{console}-{variant}.png"
            result = subprocess.run(
                [str(PREVIEW), str(staging / "menu.rml"), str(target_png),
                 str(SIZE[0]), str(SIZE[1]), *overrides],
                capture_output=True, text=True,
            )
            if result.returncode != 0:
                failures.append(f"{console}/{variant}: {result.stderr.strip()[:120]}")
                continue
            if "Could not load texture" in result.stderr:
                failures.append(f"{console}/{variant}: its artwork is not staged")
                continue
            # A scene file for each pad, because we cannot generate markup in the
            # player and read a scene file when someone changes the controller.
            beside = staging / f"scene-{variant}.rml"
            if not beside.exists():
                failures.append(f"{console}/{variant}: no {beside.name} to swap to")
                continue
            rendered += 1
            print(f"  {console:<12}{variant:<14}{beside.name}")

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


def paint(staging: Path, destination: Path, palette: str) -> tuple[list[str], dict[str, str]]:
    """Render the states of one staged palette. Return (missing names, digests)."""
    document = (staging / "menu.rml").read_text()
    destination.mkdir(parents=True, exist_ok=True)
    missing: list[str] = []
    digests: dict[str, str] = {}
    for name, state in declared_states().items():
        resolved = {i: resolve(document, i) for i in state["set"]}
        absent = [i for i, found in resolved.items() if found is None]
        if absent:
            # We do not skip it, because when the element of a state is gone,
            # someone has renamed or removed that element.
            print(
                f"  REFUSED {palette}/{name}: {', '.join(absent)} not in the document",
                file=sys.stderr,
            )
            missing.append(name)
            continue

        overrides: list[str] = []
        for element_id, properties in state["set"].items():
            for prop, value in properties.items():
                overrides += ["--set", f"{resolved[element_id]}:{prop}={value}"]

        target = destination / f"{name}.png"
        result = subprocess.run(
            [
                str(PREVIEW),
                str(staging / "menu.rml"),
                str(target),
                str(SIZE[0]),
                str(SIZE[1]),
                *overrides,
            ],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"  FAILED  {palette}/{name}: {result.stderr.strip()[:160]}", file=sys.stderr)
            missing.append(name)
            continue
        # We report a texture that does not load instead of raising, because the
        # scene is then rendered without its controller and only looks empty,
        # for example when a PNG is truncated.
        if "Could not load texture" in result.stderr:
            print(f"  FAILED  {palette}/{name}: artwork did not load", file=sys.stderr)
            missing.append(name)
            continue
        digests[name] = hashlib.sha256(target.read_bytes()).hexdigest()[:16]
        print(f"  {palette:<8}{name:<26}{state['describes']}")
    return missing, digests


def identical_names(digests: dict[str, str]) -> list[list[str]]:
    """State names with the same picture, which we compare inside one palette."""
    grouped: dict[str, list[str]] = {}
    for name, digest in digests.items():
        grouped.setdefault(digest, []).append(name)
    return [sorted(names) for names in grouped.values() if len(names) > 1]


def flat_record(record: object) -> bool:
    """The single-palette file: each state name maps to one hash string."""
    return (
        isinstance(record, dict)
        and bool(record)
        and all(isinstance(value, str) for value in record.values())
    )


def flat_digest_message() -> str:
    return (
        f"{DIGESTS.name} has the old shape: each state names one hash, and "
        "nothing names a palette. The same state in two palettes is two "
        "pictures. Re-record after looking at them:\n"
        "  python3 scripts/menu_states.py work/menu-states\n"
        "  python3 scripts/menu_states.py --record"
    )


def digest_problems(
    actual: dict[str, dict[str, str]],
    expected: object,
    *,
    complete: bool,
) -> list[str]:
    """Mismatches against a palette-keyed record. Empty when it matches.

    We do not handle a flat record here, because it has no palette to compare,
    and we report that separately in the caller.
    """
    if not isinstance(expected, dict):
        return ["the digest file is not a palette map"]
    problems: list[str] = []
    for palette, states in actual.items():
        recorded = expected.get(palette)
        if not isinstance(recorded, dict):
            problems.append(f"  MISSING {palette}: not in the recorded digests")
            continue
        for name in sorted(states):
            if recorded.get(name) != states[name]:
                problems.append(f"  CHANGED {palette}/{name}")
        for name in sorted(set(recorded) - set(states)):
            problems.append(f"  MISSING {palette}/{name}: no longer rendered")
    if complete:
        for palette in sorted(set(expected) - set(actual)):
            if isinstance(expected[palette], dict):
                problems.append(f"  MISSING {palette}: no longer rendered")
    return problems


def write_state_digests(actual: dict[str, dict[str, str]], complete: bool) -> None:
    """Write a palette-keyed digest.

    After a run with every palette we replace the file. After a run with one
    palette we update that palette and keep the others, unless the file is
    still in the old flat form, which we cannot merge and replace instead.
    """
    if complete or not DIGESTS.exists():
        record = actual
    else:
        loaded = json.loads(DIGESTS.read_text())
        if isinstance(loaded, dict) and not flat_record(loaded):
            record = {key: value for key, value in loaded.items() if isinstance(value, dict)}
            record.update(actual)
        else:
            record = actual
    DIGESTS.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, nargs="?", default=ROOT / "work/menu-states")
    parser.add_argument("--system", default="megadrive", help="which console to stage")
    parser.add_argument(
        "--variant",
        help="which of the console's controllers to draw (default: the console's own)",
    )
    parser.add_argument(
        "--palette",
        help="render this palette only; omit for every palette in desktop/designs.json",
    )
    parser.add_argument(
        "--record",
        action="store_true",
        help="record what every rendered state looks like, after looking at it",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify every rendered state still matches the recorded digest",
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

    if arguments.every_variant:
        return every_variant(arguments.output)

    if arguments.fixed_place:
        return fixed_place(arguments.output, arguments.record)

    palettes = (
        declared_palettes()
        if arguments.palette is None
        else [require_palette(arguments.palette)]
    )
    # We empty the directory, not only create it, because the old picture of a
    # renamed state would stay and show something that no longer exists as if
    # it were current. With one palette we empty only its directory, so in a
    # quick iteration we keep the pictures of the others.
    if arguments.palette is None and arguments.output.exists():
        shutil.rmtree(arguments.output)
    arguments.output.mkdir(parents=True, exist_ok=True)

    missing: list[str] = []
    digests: dict[str, dict[str, str]] = {}
    rendered = 0
    twins: list[tuple[str, list[str]]] = []
    for palette in palettes:
        if arguments.palette is not None:
            palette_dir = arguments.output / palette
            if palette_dir.exists():
                shutil.rmtree(palette_dir)
        workspace = ROOT / "work/menu-states-staging" / palette
        if workspace.exists():
            shutil.rmtree(workspace)
        staging = stage(arguments.system, workspace, arguments.variant, palette)
        absent, pictured = paint(staging, arguments.output / palette, palette)
        missing.extend(f"{palette}/{name}" for name in absent)
        digests[palette] = pictured
        rendered += len(pictured)
        # Two states with the same picture are one state with two names. We compare
        # within this palette, because one state gives a picture per palette.
        # Hover, keyboard focus and held-down must differ, so that a player using
        # the arrow keys can see which option they would choose with a press.
        for names in identical_names(pictured):
            twins.append((palette, names))
            print(
                f"  IDENTICAL {palette}: {', '.join(names)}: the same picture",
                file=sys.stderr,
            )

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

    if arguments.record:
        write_state_digests(digests, complete=arguments.palette is None)
        recorded = sum(len(states) for states in digests.values())
        print(
            f"\nrecorded {recorded} state digests across {len(digests)} palette(s) "
            f"-> {DIGESTS.name}"
        )
        return 0

    if arguments.check:
        if not DIGESTS.exists():
            raise SystemExit(f"no recorded states at {DIGESTS}; run --record first")
        expected = json.loads(DIGESTS.read_text())
        if flat_record(expected):
            print(f"\n{flat_digest_message()}", file=sys.stderr)
            return 1
        problems = digest_problems(
            digests, expected, complete=arguments.palette is None
        )
        if problems:
            for line in problems:
                print(line, file=sys.stderr)
            print(
                "\nA menu state looks different. Look at the pictures before "
                "re-recording:\n  python3 scripts/menu_states.py work/menu-states\n"
                "  python3 scripts/menu_states.py --record",
                file=sys.stderr,
            )
            return 1
        print(f"\n{len(palettes)} palette(s), {rendered} pictures unchanged")
        return 0

    print(f"\n{rendered} pictures across {len(palettes)} palette(s) -> {arguments.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
