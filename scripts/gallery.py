"""Collect the work of each checkout into one folder.

Pictures that are the same in every checkout would hide the work, so we
collect only pictures that differ from the canonical tree.

From each checkout we collect two things and nothing else:

  * Its photographs: what we took with `scripts/menu_shots.py` from an
    exported game in `work/menu-shots`, and any other picture whose path is in
    the checkout's own report, such as a game frame, a zoom or a comparison.
    A checkout with no photographs adds none.

    A report starts at `work/<NAME>.md`, beside pictures in a scratch
    directory listed in .gitignore. When we merge the branch, we move both to
    `docs/reports/`, so the claim and its proof remain after we remove the
    worktree and anyone with the repository can check them.
  * The screens that differ. We draw every declared menu state, and the
    controls screen with each pad a player can pick, in every checkout, and
    compare each with the same picture drawn in the canonical tree. We drop
    identical ones. A checkout without changes to the menu's appearance adds
    nothing, and one with a new screen or pad adds exactly what changed.

    python3 scripts/gallery.py                 # collect
    python3 scripts/gallery.py --check         # is anything claimed but absent?

This does not show that a picture is correct, or that a photograph came from
an exported game and not from the offscreen renderer. It shows that every
picture in a checkout's report exists, and that nothing here is a duplicate.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GALLERY = ROOT / "work/gallery"
CANONICAL = ROOT
TREES = sorted(ROOT.parent.glob("rominabox*"))

# The folder for photographs of an exported game from scripts/menu_shots.py.
SHOTS = "work/menu-shots"

# A photograph and a rendered screen can have the same name ("controls" is a
# state we render and also a picture we take through the game), so we keep
# them apart and a render never overwrites a photograph of an exported game.
PHOTO = "photo"
SCREEN = "screen"
# The names of the files we write in this script. The pattern also matches
# the names from earlier versions of this script, so we clear those pictures
# too. We leave alone a file that is not a picture or not named this way.
NAME = re.compile(r"^[a-z0-9-]+--.+\.png$")


def gallery_name(track: str, kind: str, picture: Path) -> str:
    """The name of a picture in the gallery.

    We put the source folder in the name, because two different pictures
    can have the same stem: a photograph called controls.png and a rendered
    state called controls.png, or two photographs called pause-menu.png, one
    of the pause menu and one of the pause menu with SHADERS in the corner.
    """
    where = picture.parent.name
    return f"{track}--{kind}--{where}--{picture.stem}.png"

# In a report we give the path of each picture from the checkout root, in
# the scratch directory or, after the merge, beside the report.
NAMED = re.compile(r"(?:work|docs/reports)/[A-Za-z0-9._/-]+\.png")


def track_of(tree: Path) -> str:
    return tree.name.replace("rominabox-", "") if tree.name != "rominabox" else "main"


def claimed(tree: Path) -> list[str]:
    """Pictures whose paths are in a checkout's own report."""
    reports = sorted((tree / "work").glob("*.md")) if (tree / "work").is_dir() else []
    if (merged := tree / "docs/reports").is_dir():
        reports += sorted(merged.glob("*.md"))
    seen: list[str] = []
    for report in reports:
        for path in NAMED.findall(report.read_text(errors="replace")):
            if path not in seen:
                seen.append(path)
    return seen


def photographs(tree: Path) -> list[Path]:
    """Every picture we took in a checkout: its shots directory and the pictures in its report."""
    found: list[Path] = sorted((tree / SHOTS).glob("*.png")) if (tree / SHOTS).is_dir() else []
    for merged in sorted((tree / "docs/reports").glob("*/*.png")):
        if merged not in found:
            found.append(merged)
    for path in claimed(tree):
        picture = tree / path
        if picture.is_file() and picture not in found:
            found.append(picture)
    return found


# Two renders, because they show different work. The states are one console's
# menu in every condition, and the variants are the controls screen with each
# pad a player can pick. A new pad in a checkout changes only the second.
RENDERS = {
    "states": [],
    "variants": ["--every-variant"],
}


def draw(tree: Path) -> tuple[dict[str, Path], list[str]]:
    """Draw every picture we can draw in this checkout, keyed by picture name.

    When we cannot draw in a checkout, we report it instead of passing over
    it, so a checkout with a broken renderer does not appear to have no
    changed screens.
    """
    drawn: dict[str, Path] = {}
    broken: list[str] = []
    for kind, flags in RENDERS.items():
        out = tree / f"work/gallery-render/{kind}"
        done = subprocess.run(
            [sys.executable, "scripts/menu_states.py", *flags, str(out)],
            cwd=tree, capture_output=True, text=True,
        )
        if done.returncode != 0:
            reason = done.stderr.strip()[-160:]
            print(f"  {track_of(tree)}: no {kind} — {reason}")
            broken.append(f"{track_of(tree)} cannot draw its {kind}: {reason}")
            continue
        # We render the states into a directory per palette, so we put the
        # palette in the picture's name too. Amber and Green are different
        # pictures and would otherwise have the same name as Blue's.
        for picture in sorted(out.glob("**/*.png")):
            where = picture.parent.name
            name = picture.stem if where == kind else f"{where}-{picture.stem}"
            drawn[name] = picture
    return drawn, broken


def changed(drawn: dict[str, Path], baseline: dict[str, Path] | None) -> list[Path]:
    """The pictures that differ in this checkout from the canonical tree."""
    if baseline is None:
        return list(drawn.values())
    return [
        picture
        for name, picture in drawn.items()
        if name not in baseline
        or baseline[name].read_bytes() != picture.read_bytes()
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="report what is claimed but absent")
    arguments = parser.parse_args()

    if arguments.check:
        return check()

    if GALLERY.is_symlink():
        print(f"{GALLERY} is a link. Refusing to clear it: what would be removed "
              f"is not in the place this script owns.", file=sys.stderr)
        return 1
    GALLERY.mkdir(parents=True, exist_ok=True)
    collected: dict[str, Path] = {}
    # We never add the same picture twice, from whichever checkout. Several
    # checkouts can contain the same commit that changes the menu, and two
    # consoles share a pad, so without this the folder would fill with
    # repeats. We record beside the name the other checkouts that have the
    # picture, because it is one picture from the work of several checkouts.
    once: dict[bytes, str] = {}
    shared: dict[str, list[str]] = {}
    broken: list[str] = []

    def take(name: str, picture: Path) -> None:
        body = picture.read_bytes()
        if (already := once.get(body)) is not None:
            if already != name:
                shared.setdefault(already, []).append(name)
            return
        if (standing := collected.get(name)) is not None:
            # Two different pictures with one name are an error, never a silent
            # overwrite, so we never replace a photograph of a game with a drawing.
            raise SystemExit(
                f"{name} would hold two different pictures:\n"
                f"  {standing}\n  {picture}\n"
                f"Their names collide. gallery_name decides what a picture is "
                f"called; it has to tell these apart."
            )
        once[body] = name
        collected[name] = picture

    # We put the canonical tree first explicitly. It also sorts first, but only
    # because every worktree's name starts with its name, and otherwise a tree
    # with another name would become the baseline we compare everything with.
    order = [CANONICAL] + [tree for tree in TREES if tree != CANONICAL]
    base, base_broken = draw(CANONICAL)
    broken += base_broken
    for tree in order:
        track = track_of(tree)
        before = len(collected)
        for picture in photographs(tree):
            take(gallery_name(track, PHOTO, picture), picture)
        photographed = len(collected) - before
        if tree == CANONICAL:
            drawn = base
        else:
            drawn, trouble = draw(tree)
            broken += trouble
        different = changed(drawn, None if tree == CANONICAL else base)
        before = len(collected)
        for picture in different:
            take(gallery_name(track, SCREEN, picture), picture)
        note = f"{photographed} photograph(s)"
        note += f", {len(collected) - before} screen(s) it draws "
        note += "on its own" if tree == CANONICAL else "differently"
        print(f"  {track:<16}{note}")

    # We remove only what we write in this script, by name, one at a time. We
    # leave alone a file from another program or a person.
    stale = [
        entry for entry in GALLERY.iterdir()
        if entry.is_file()
        and not entry.is_symlink()
        and NAME.match(entry.name)
        and entry.name not in collected
    ]
    for entry in stale:
        entry.unlink()
    if stale:
        print(f"  cleared {len(stale)} picture(s) nothing produced")

    for name, picture in collected.items():
        shutil.copyfile(picture, GALLERY / name)
    (GALLERY / "SHARED.txt").write_text(
        "Pictures several tracks produced identically. The picture is in the\n"
        "gallery once, under the first name; the others drew the same thing.\n\n"
        + "".join(
            f"{name}\n" + "".join(f"    also {other}\n" for other in others)
            for name, others in sorted(shared.items())
        )
    )
    if shared:
        print(f"  {sum(len(v) for v in shared.values())} picture(s) another track "
              f"had already drawn; see SHARED.txt")
    if broken:
        for entry in broken:
            print(f"  BROKEN  {entry}", file=sys.stderr)
    print(f"\n{len(collected)} pictures -> {GALLERY}")
    return 0


def check() -> int:
    """Whether every picture from a checkout is in the folder.

    We compare bytes, not filenames. A check that a picture in a report exists
    in the checkout's own tree would always pass, because someone wrote the
    report after making the picture, and a check that the checkout has any
    file in the gallery would pass with one unrelated picture.
    """
    inside = {picture.read_bytes() for picture in GALLERY.glob("*.png")}
    absent: list[str] = []
    missing: list[str] = []
    for tree in TREES:
        track = track_of(tree)
        for path in claimed(tree):
            if not (tree / path).is_file():
                absent.append(f"{track}: {path} is named in its report and is not there")
        for picture in photographs(tree):
            if picture.read_bytes() not in inside:
                missing.append(f"{track}: {picture.relative_to(tree)} never reached the gallery")

    same: dict[bytes, str] = {}
    repeats: list[str] = []
    for picture in sorted(GALLERY.glob("*.png")):
        body = picture.read_bytes()
        if body in same:
            repeats.append(f"{picture.name} is the same picture as {same[body]}")
        else:
            same[body] = picture.name

    for entry in absent:
        print(f"  CLAIMED BUT ABSENT  {entry}", file=sys.stderr)
    for entry in missing:
        print(f"  NOT COLLECTED  {entry}", file=sys.stderr)
    for entry in repeats[:12]:
        print(f"  DUPLICATE  {entry}", file=sys.stderr)
    if len(repeats) > 12:
        print(f"  ...{len(repeats)} duplicate(s) in all", file=sys.stderr)
    if absent or missing or repeats:
        print(
            "\nA report points at a picture that is not there, a picture a track "
            "produced never arrived, or the same picture is in here many times "
            "over, which is how the last two galleries buried the work:"
            "\n  python3 scripts/gallery.py",
            file=sys.stderr,
        )
        return 1
    print(f"every picture a track produced is here, once: {len(inside)} of them")
    return 0


if __name__ == "__main__":
    sys.exit(main())
