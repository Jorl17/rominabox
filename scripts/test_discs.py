"""Check the image count of a multi-disc game in the core, and that choosing
the second image changes the disc index.

We show a disc entry in the menu only when get_num_images of the core returns
more than one, and choosing a row there calls CMD_EVENT_DISK_INDEX. In this
harness, outside RetroArch, we cannot call that command, so we use the same
disk callbacks in the same order: eject, then set the index. We read the
index before and after from the core, and never make up a number here.

A cartridge has no disc tray, and the tray of a single CHD has one image, so
neither is a multi-disc game. The playlist is one, and after someone chooses
disc 2 (the second image), the index is 1.

    python3 scripts/test_discs.py
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = Path("/Users/mariowilde/Downloads/roms/Ape Escape (Europe).chd")
CARTRIDGE = Path(
    "/Users/mariowilde/Downloads/roms/Sonic Advance (Europe) (En,Ja,Fr,De,Es).gba"
)
# The label from the core is this file name without the extension. We put
# the disc number at the end because we shorten long names in the middle in
# the menu to keep the end of a name visible. A name that ended before the
# number would not test that.
STEM = "Ape Escape (Europe) (En,Fr,De,Es,It,Nl,Pt,Sv,No,Da,Fi,Pl)"
FIXTURE = ROOT / "work/i5-discs"
CORES = ROOT / "desktop/src-tauri/resources/runtime/cores"
HARNESS_SOURCE = ROOT / "scripts/native_runtime/frame_harness.c"
LIBRETRO = ROOT / "vendor/retroarch/libretro-common/include"
HARNESS = FIXTURE / "frame_harness"


def copy_disc(number: int) -> Path:
    destination = FIXTURE / f"{STEM} (Disc {number}).chd"
    if not SOURCE.is_file():
        raise SystemExit(f"the source disc is not at {SOURCE}")
    FIXTURE.mkdir(parents=True, exist_ok=True)
    if (
        not destination.is_file()
        or destination.stat().st_size != SOURCE.stat().st_size
    ):
        shutil.copy2(SOURCE, destination)
    return destination


def write_playlist(name: str, count: int) -> Path:
    path = FIXTURE / name
    lines = [f"{STEM} (Disc {number}).chd" for number in range(1, count + 1)]
    path.write_text("\n".join(lines) + "\n")
    return path


def compile_harness() -> None:
    FIXTURE.mkdir(parents=True, exist_ok=True)
    compiled = subprocess.run(
        ["cc", "-O2", f"-I{LIBRETRO}", "-o", str(HARNESS), str(HARNESS_SOURCE)],
        capture_output=True,
        text=True,
    )
    if compiled.returncode != 0:
        sys.stderr.write(compiled.stderr)
        raise SystemExit("frame_harness did not compile")


def core_for(content: Path) -> Path:
    name = (
        "mgba_libretro.dylib"
        if content.suffix == ".gba"
        else "pcsx_rearmed_libretro.dylib"
    )
    path = CORES / name
    if not path.is_file():
        raise SystemExit(f"the shipped core is not in the runtime kit: {path}")
    return path


def phases_of(text: str) -> dict[str, dict]:
    found: dict[str, dict] = {}
    current = None
    for line in text.splitlines():
        if line.startswith("disc-phase "):
            current = line.split(" ", 1)[1]
            found[current] = {"images": None, "index": None, "labels": []}
            continue
        if current is None:
            continue
        if line.startswith("disc-images "):
            found[current]["images"] = int(line.split()[1])
        elif line.startswith("disc-index "):
            found[current]["index"] = int(line.split()[1])
        elif line.startswith("disc-label "):
            parts = line.split(" ", 2)
            found[current]["labels"].append(parts[2] if len(parts) > 2 else "")
    return found


def run(content: Path, disc: int | None = None) -> dict[str, dict]:
    command = [
        str(HARNESS),
        "--core",
        str(core_for(content)),
        "--content",
        str(content),
        "--frames",
        "1",
        "--system-dir",
        str(FIXTURE / "system"),
        "--save-dir",
        str(FIXTURE / "save"),
    ]
    if disc is not None:
        command += ["--disc", str(disc)]
    (FIXTURE / "system").mkdir(parents=True, exist_ok=True)
    (FIXTURE / "save").mkdir(parents=True, exist_ok=True)
    completed = subprocess.run(command, capture_output=True, text=True, timeout=180)
    if completed.returncode != 0:
        sys.stderr.write(completed.stderr)
        raise SystemExit(
            f"the core refused {content.name} (exit {completed.returncode})"
        )
    phases = phases_of(completed.stdout)
    if "before" not in phases or "after" not in phases:
        sys.stderr.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        raise SystemExit(f"{content.name} produced no disc report")
    return phases


def main() -> None:
    if not CARTRIDGE.is_file():
        raise SystemExit(f"the cartridge is not at {CARTRIDGE}")
    compile_harness()
    for number in range(1, 5):
        copy_disc(number)
    single = FIXTURE / f"{STEM} (Disc 1).chd"
    two = write_playlist("two-discs.m3u", 2)
    four = write_playlist("four-discs.m3u", 4)

    cartridge = run(CARTRIDGE)
    images = cartridge["before"]["images"]
    if images is None or images > 1:
        raise SystemExit(
            f"Sonic Advance reported {images} images; a cartridge is not a multi-disc game"
        )

    one = run(single)
    if one["before"]["images"] != 1:
        raise SystemExit(
            f"a single disc reported {one['before']['images']} images"
        )

    swapped = run(two, disc=1)
    before = swapped["before"]["index"]
    after = swapped["after"]["index"]
    print(f"two-disc before index {before}, after index {after}")
    if swapped["before"]["images"] != 2:
        raise SystemExit(
            f"a two-disc playlist reported {swapped['before']['images']} images"
        )
    if before != 0 or after != 1:
        raise SystemExit(
            f"after choosing disc 2 the core still reports image index {after}, not 1"
        )

    listed = run(four)
    labels = listed["before"]["labels"]
    print("four-disc labels:")
    for label in labels:
        print(f"  {label}")
    if listed["before"]["images"] != 4 or len(labels) != 4:
        raise SystemExit(
            f"a four-disc playlist reported {listed['before']['images']} images"
        )
    for number, label in enumerate(labels, start=1):
        if f"(Disc {number})" not in label:
            raise SystemExit(
                f"disc {number} label lost its number: {label}"
            )


if __name__ == "__main__":
    main()
