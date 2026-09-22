"""Collect the work of each checkout into one folder.

Pictures that are the same in every checkout would hide the work, so we
collect only pictures that differ from the canonical tree.

From each checkout we collect two things and nothing else:

  * Its photographs: what we took with `scripts/menu_shots.py` from an
    exported game in `work/menu-shots`, and any other picture whose path is in
    the checkout's own report in `work/*.md`, such as a game frame, a zoom or
    a comparison. A checkout with no photographs adds none.
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

# In a report we give the path of each picture from the checkout root.
NAMED = re.compile(r"work/[A-Za-z0-9._/-]+\.png")


def track_of(tree: Path) -> str:
    return tree.name.replace("rominabox-", "") if tree.name != "rominabox" else "main"


def claimed(tree: Path) -> list[str]:
    """Pictures whose paths are in a checkout's own report."""
    reports = sorted((tree / "work").glob("*.md")) if (tree / "work").is_dir() else []
    seen: list[str] = []
    for report in reports:
        for path in NAMED.findall(report.read_text(errors="replace")):
            if path not in seen:
                seen.append(path)
    return seen


def photographs(tree: Path) -> list[Path]:
    """Every picture we took in a checkout: its shots directory and the pictures in its report."""
    found: list[Path] = sorted((tree / SHOTS).glob("*.png")) if (tree / SHOTS).is_dir() else []
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


def draw(tree: Path) -> dict[str, Path]:
    """Draw every picture we can draw in this checkout, keyed by picture name."""
    drawn: dict[str, Path] = {}
    for kind, flags in RENDERS.items():
        out = tree / f"work/gallery-render/{kind}"
        done = subprocess.run(
            [sys.executable, "scripts/menu_states.py", *flags, str(out)],
            cwd=tree, capture_output=True, text=True,
        )
        if done.returncode != 0:
            print(f"  {track_of(tree)}: no {kind} — {done.stderr.strip()[-160:]}")
            continue
        for picture in sorted(out.glob("*.png")):
            drawn[picture.stem] = picture
    return drawn


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
        absent: list[str] = []
        empty: list[str] = []
        # We leave out the same picture under a second name as a duplicate.
        same: dict[bytes, str] = {}
        repeats: list[str] = []
        for picture in sorted(GALLERY.glob("*.png")):
            body = picture.read_bytes()
            if body in same:
                repeats.append(f"{picture.name} is the same picture as {same[body]}")
            else:
                same[body] = picture.name
        for tree in TREES:
            track = track_of(tree)
            for path in claimed(tree):
                if not (tree / path).is_file():
                    absent.append(f"{track}: {path}")
            if photographs(tree) and not any(GALLERY.glob(f"{track}--*.png")):
                empty.append(track)
        for entry in absent:
            print(f"  CLAIMED BUT ABSENT  {entry}", file=sys.stderr)
        for track in empty:
            print(f"  PHOTOGRAPHED BUT NOT COLLECTED  {track}", file=sys.stderr)
        for entry in repeats[:12]:
            print(f"  DUPLICATE  {entry}", file=sys.stderr)
        if repeats:
            print(f"  ...{len(repeats)} duplicate(s) in all", file=sys.stderr)
        if absent or empty or repeats:
            print(
                "\nA report points at a picture that is not there, or a track's "
                "photographs never reached the gallery. Someone looking for that "
                "work will not find it, or the same picture is in here many "
                "times over, which is how the last two galleries buried it:"
                "\n  python3 scripts/gallery.py",
                file=sys.stderr,
            )
            return 1
        print(f"every claimed picture exists; {len(list(GALLERY.glob('*.png')))} in the gallery")
        return 0

    GALLERY.mkdir(parents=True, exist_ok=True)
    collected: dict[str, Path] = {}
    # We never add the same picture twice, from whichever checkout. Several
    # checkouts can contain the same commit that changes the menu, and two
    # consoles share a pad, so without this the folder would fill with
    # repeats. We visit the canonical tree first, so we never credit a
    # picture it already has to another checkout as that checkout's work.
    once: dict[bytes, str] = {}
    shared: list[str] = []

    def take(name: str, picture: Path) -> None:
        body = picture.read_bytes()
        if (already := once.get(body)) is not None:
            if already != name:
                shared.append(f"{name} would have been another copy of {already}")
            return
        once[body] = name
        collected[name] = picture

    base = draw(CANONICAL)
    for tree in TREES:
        track = track_of(tree)
        taken = photographs(tree)
        before = len(collected)
        for picture in taken:
            take(f"{track}--{picture.stem}.png", picture)
        photographed = len(collected) - before
        drawn = base if tree == CANONICAL else draw(tree)
        different = changed(drawn, None if tree == CANONICAL else base)
        before = len(collected)
        for picture in different:
            take(f"{track}--{picture.stem}.png", picture)
        note = f"{photographed} photograph(s)"
        note += f", {len(collected) - before} screen(s) it draws "
        note += "on its own" if tree == CANONICAL else "differently"
        print(f"  {track:<16}{note}")

    # We remove every file we did not just collect, so a renamed picture does
    # not remain and the folder contains only the current work. We remove each
    # file by name, one at a time, never recursively.
    stale = [
        entry for entry in GALLERY.iterdir()
        if entry.is_file() and entry.name not in collected
    ]
    for entry in stale:
        entry.unlink()
    if stale:
        print(f"  cleared {len(stale)} picture(s) nothing produced")

    if shared:
        print(f"  {len(shared)} picture(s) another track had already drawn, left out")
    for name, picture in collected.items():
        shutil.copyfile(picture, GALLERY / name)
    print(f"\n{len(collected)} pictures -> {GALLERY}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
