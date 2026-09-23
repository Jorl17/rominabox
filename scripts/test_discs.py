"""Check the image count of a multi-disc game in the core, and that choosing
the second image changes the disc index.

We show a disc entry in the menu only when get_num_images of the core returns
more than one, and choosing a row there calls CMD_EVENT_DISK_INDEX. In this
harness, outside RetroArch, we cannot call that command, so we use the same
disk callbacks in the same order: eject, then set the index. We read the
index before and after from the core, and never make up a number here.

A cartridge has no disc tray, and the tray of a single generated cue has one
image, so neither is a multi-disc game. The playlist is one, and after
someone chooses disc 2 (the second image), the index is 1.

In PCSX ReARMed, an m3u counts only after the first image passes CheckCdrom
and LoadCdrom. We read what these checks require in the core's code and then
tried it: a MODE1/2048 cue whose file is a multiple of 2048 bytes and does
not start with the raw sync word, an ISO9660 primary descriptor at sector 16,
a SYSTEM.CNF that lists PSX.EXE;1, and a PS-X EXE header with text size 0, so
that no program is copied. The HLE BIOS is enough. We run no frames in the
harness, so the stub never runs. Once the first image is loaded,
get_num_images is the number of lines in the playlist.

    python3 scripts/test_discs.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARTRIDGE = ROOT / "scripts/fixtures/test-game.gbc"
# The label from the core is this file name without the extension. We put
# the disc number at the end because we shorten long names in the middle in
# the menu to keep the end of a name visible. A name that ended before the
# number would not test that.
STEM = "ROM in a Box (Europe) (En,Fr,De,Es,It,Nl,Pt,Sv,No,Da,Fi,Pl)"
FIXTURE = ROOT / "work/i5-discs"
CORES = ROOT / "desktop/src-tauri/resources/runtime/cores"
HARNESS_SOURCE = ROOT / "scripts/native_runtime/frame_harness.c"
LIBRETRO = ROOT / "vendor/retroarch/libretro-common/include"
HARNESS = FIXTURE / "frame_harness"
SECTOR = 2048


def both_endian(value: int, width: int) -> bytes:
    return value.to_bytes(width, "little") + value.to_bytes(width, "big")


def directory_record(name: bytes, extent: int, size: int, flags: int) -> bytes:
    record = bytearray(33 + len(name))
    record[2:10] = both_endian(extent, 4)
    record[10:18] = both_endian(size, 4)
    record[25] = flags
    record[28:32] = both_endian(1, 2)
    record[32] = len(name)
    record[33:] = name
    if len(record) % 2:
        record.append(0)
    record[0] = len(record)
    return bytes(record)


def disc_image() -> bytes:
    """Return one data track with nothing to execute, for the core to load.

    Sector 16 is the volume descriptor. The root directory record in it
    points to sector 18. SYSTEM.CNF and the executable header follow. The
    text size of the executable is 0, so LoadCdrom returns after the header.
    """
    configuration = (
        b"BOOT = cdrom:\\PSX.EXE;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFFF0\r\n"
    )
    executable = bytearray(SECTOR)
    executable[0:8] = b"PS-X EXE"
    executable[0x10:0x14] = (0x80010000).to_bytes(4, "little")
    executable[0x18:0x1C] = (0x80010000).to_bytes(4, "little")
    executable[0x30:0x34] = (0x801FFF00).to_bytes(4, "little")

    root = b"".join(
        (
            directory_record(b"\x00", 18, SECTOR, 2),
            directory_record(b"\x01", 18, SECTOR, 2),
            directory_record(b"SYSTEM.CNF;1", 19, len(configuration), 0),
            directory_record(b"PSX.EXE;1", 20, len(executable), 0),
        )
    )
    descriptor = bytearray(SECTOR)
    descriptor[0] = 1
    descriptor[1:6] = b"CD001"
    descriptor[6] = 1
    descriptor[40:72] = b"ROMINABOX".ljust(32)
    descriptor[80:88] = both_endian(21, 4)
    descriptor[120:124] = both_endian(1, 2)
    descriptor[124:128] = both_endian(1, 2)
    descriptor[128:132] = both_endian(SECTOR, 2)
    root_record = directory_record(b"\x00", 18, SECTOR, 2)
    descriptor[156 : 156 + len(root_record)] = root_record
    terminator = bytearray(SECTOR)
    terminator[0] = 255
    terminator[1:6] = b"CD001"
    terminator[6] = 1

    image = bytearray(21 * SECTOR)
    image[16 * SECTOR : 17 * SECTOR] = descriptor
    image[17 * SECTOR : 18 * SECTOR] = terminator
    image[18 * SECTOR : 18 * SECTOR + len(root)] = root
    image[19 * SECTOR : 19 * SECTOR + len(configuration)] = configuration
    image[20 * SECTOR : 21 * SECTOR] = executable
    return bytes(image)


def write_disc(number: int, image: bytes) -> None:
    stem = f"{STEM} (Disc {number})"
    (FIXTURE / f"{stem}.bin").write_bytes(image)
    (FIXTURE / f"{stem}.cue").write_text(
        f'FILE "{stem}.bin" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\n'
    )


def write_playlist(name: str, count: int) -> Path:
    path = FIXTURE / name
    lines = [f"{STEM} (Disc {number}).cue" for number in range(1, count + 1)]
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
    # Without its boot ROM, Gambatte does not load this cartridge. In mGBA it
    # loads with no disc tray, which is what we check for a cartridge.
    name = (
        "mgba_libretro.dylib"
        if content.suffix == ".gbc"
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
        "0",
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
    image = disc_image()
    for number in range(1, 5):
        write_disc(number, image)
    single = FIXTURE / f"{STEM} (Disc 1).cue"
    two = write_playlist("two-discs.m3u", 2)
    four = write_playlist("four-discs.m3u", 4)

    cartridge = run(CARTRIDGE)
    images = cartridge["before"]["images"]
    if images is None or images > 1:
        raise SystemExit(
            f"the test cartridge reported {images} images; a cartridge is not a multi-disc game"
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
