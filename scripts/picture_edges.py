"""Check that a hard edge in the picture of a core stays a hard edge.

In an exported game we set the options of a core as RetroArch does: the
default that the core declared, overridden by the `pixels` entries on the
component of that core. The declared default of Nestopia replaces the picture
with a composite-video reconstruction, which adds a fringe of new colours
along every vertical edge. In an export we write the `pixels` entries instead.

We build a cartridge that is black on one side of a vertical line and white
on the other, run it on the core selected in the catalogue for that
extension, and count colours along the edge. Two colours is a hard edge.
Anything else was resampled.

    python3 scripts/picture_edges.py
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import frame_harness  # noqa: E402
import scratch  # noqa: E402
from core_source import core as local_core  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
PACKAGES = ROOT / "integrations/consoles"
EXTENSION = "nes"


def load_json(path: Path) -> dict:
    return json.loads(path.read_text())


def console_for_extension(extension: str) -> dict:
    matches = []
    for path in sorted(PACKAGES.glob("*/console.json")):
        console = load_json(path)
        if extension in console.get("content", {}).get("extensions", []):
            matches.append(console)
    if len(matches) != 1:
        ids = ", ".join(console["id"] for console in matches) or "none"
        raise SystemExit(f"{extension} must belong to one console, found: {ids}")
    return matches[0]


def component_of(console: dict) -> dict:
    bindings = console.get("cores") or []
    if not bindings:
        raise SystemExit(f"{console['id']} declares no core")
    wanted = bindings[0]["component"]
    for path in PACKAGES.glob("*/components/*.json"):
        component = load_json(path)
        if component.get("id") == wanted:
            return component
    raise SystemExit(f"no component {wanted}")


def edge_rom() -> bytes:
    """A 16 KB NROM program that draws one vertical edge and one horizontal edge.

    The upper half is black on the left and white on the right. The lower half
    swaps them, so a filtered picture cannot hide the fringe by cropping one
    border. Tile 0 is colour 0 and tile 1 is colour 1.
    """
    code = bytearray()
    labels: dict[str, int] = {}
    fixups: list[tuple] = []

    def emit(*values: int) -> None:
        code.extend(values)

    def label(name: str) -> None:
        labels[name] = len(code)

    def rel(opcode: int, name: str) -> None:
        emit(opcode, 0)
        fixups.append(("rel", len(code) - 1, name))

    label("nmi")
    emit(0x40)
    label("irq")
    emit(0x40)
    label("reset")
    emit(0x78, 0xD8)
    emit(0xA2, 0x40)
    emit(0x8E, 0x17, 0x40)
    emit(0xA2, 0xFF)
    emit(0x9A)
    emit(0xE8)
    emit(0x8E, 0x00, 0x20)
    emit(0x8E, 0x01, 0x20)
    emit(0x8E, 0x10, 0x40)
    emit(0x2C, 0x02, 0x20)
    label("wait1")
    emit(0x2C, 0x02, 0x20)
    rel(0x10, "wait1")
    emit(0x8A)
    label("clear")
    for page in range(8):
        emit(0x9D, 0x00, page)
    emit(0xE8)
    rel(0xD0, "clear")
    emit(0x2C, 0x02, 0x20)
    label("wait2")
    emit(0x2C, 0x02, 0x20)
    rel(0x10, "wait2")
    emit(0xA9, 0x3F)
    emit(0x8D, 0x06, 0x20)
    emit(0xA9, 0x00)
    emit(0x8D, 0x06, 0x20)
    for colour in (0x0F, 0x30, 0x16, 0x1A):
        emit(0xA9, colour)
        emit(0x8D, 0x07, 0x20)
    emit(0xA9, 0x20)
    emit(0x8D, 0x06, 0x20)
    emit(0xA9, 0x00)
    emit(0x8D, 0x06, 0x20)

    def half(name: str, tile: int) -> None:
        label(name)
        emit(0xA9, tile)
        emit(0x8D, 0x07, 0x20)
        emit(0xCA)
        rel(0xD0, name)

    emit(0xA0, 15)
    label("top")
    emit(0xA2, 16)
    half("top_left", 0)
    emit(0xA2, 16)
    half("top_right", 1)
    emit(0x88)
    rel(0xD0, "top")
    emit(0xA0, 15)
    label("bot")
    emit(0xA2, 16)
    half("bot_left", 1)
    emit(0xA2, 16)
    half("bot_right", 0)
    emit(0x88)
    rel(0xD0, "bot")
    emit(0xA2, 64)
    label("attr")
    emit(0xA9, 0x00)
    emit(0x8D, 0x07, 0x20)
    emit(0xCA)
    rel(0xD0, "attr")
    emit(0xA9, 0x00)
    emit(0x8D, 0x05, 0x20)
    emit(0x8D, 0x05, 0x20)
    emit(0xA9, 0x80)
    emit(0x8D, 0x00, 0x20)
    emit(0xA9, 0x0E)
    emit(0x8D, 0x01, 0x20)
    label("forever")
    emit(0x4C, 0x00, 0x00)
    fixups.append(("abs", len(code) - 2, "forever"))

    for kind, at, name in fixups:
        if kind == "abs":
            address = 0x8000 + labels[name]
            code[at] = address & 0xFF
            code[at + 1] = address >> 8
        else:
            offset = labels[name] - (at + 1)
            if not -128 <= offset <= 127:
                raise SystemExit(f"branch to {name} does not fit")
            code[at] = offset & 0xFF

    program = bytearray(16384)
    if len(code) > 16384 - 6:
        raise SystemExit("edge program does not fit a 16 KB bank")
    program[: len(code)] = code

    def vector(name: str) -> bytes:
        address = 0x8000 + labels[name]
        return bytes((address & 0xFF, address >> 8))

    program[-6:-4] = vector("nmi")
    program[-4:-2] = vector("reset")
    program[-2:] = vector("irq")

    tiles = bytearray(8192)
    for row in range(8):
        tiles[16 + row] = 0xFF
    header = bytearray(b"NES\x1a")
    header += bytes((1, 1, 1)) + bytes(9)
    return bytes(header) + bytes(program) + bytes(tiles)


def load_ppm(path: Path) -> tuple[int, int, list[bytes]]:
    data = path.read_bytes()
    if not data.startswith(b"P6\n"):
        raise SystemExit(f"{path} is not a PPM frame")
    header, _, body = data[3:].partition(b"\n255\n")
    width, height = (int(part) for part in header.split())
    if len(body) != width * height * 3:
        raise SystemExit(f"{path} is {width}x{height} but holds {len(body)} bytes")
    pixels = [body[index : index + 3] for index in range(0, len(body), 3)]
    return width, height, pixels


def edge_colours(width: int, height: int, pixels: list[bytes]) -> tuple[list[bytes], list[tuple[bytes, int]]]:
    """Colours and runs on a row through the upper half, clear of the horizontal edge."""
    y = height // 4
    row = pixels[y * width : (y + 1) * width]
    colours: list[bytes] = []
    runs: list[tuple[bytes, int]] = []
    for colour in row:
        if colour not in colours:
            colours.append(colour)
        if not runs or runs[-1][0] != colour:
            runs.append((colour, 1))
        else:
            previous, count = runs[-1]
            runs[-1] = (previous, count + 1)
    return colours, runs


def main() -> int:
    console = console_for_extension(EXTENSION)
    component = component_of(console)
    artifact = component.get("artifacts", {}).get("macos-arm64")
    if not artifact:
        raise SystemExit(f"{component['id']} has no macos-arm64 artifact")
    core = local_core(artifact)

    overrides = component.get("pixels") or []
    with scratch.scratch("rominabox-picture-edges-") as temporary:
        root = Path(temporary)
        harness = frame_harness.compile_to(root / "frame_harness")
        rom = root / "edge.nes"
        shot = root / "edge.ppm"
        rom.write_bytes(edge_rom())
        command = [
            str(harness),
            "--core",
            str(core),
            "--content",
            str(rom),
            "--frames",
            "8",
            "--shot",
            f"8:{shot}",
            "--system-dir",
            str(root),
            "--save-dir",
            str(root),
        ]
        for override in overrides:
            command += ["--option", f"{override['key']}={override['value']}"]
        ran = subprocess.run(command, capture_output=True, text=True)
        if ran.returncode != 0 or not shot.is_file():
            sys.stderr.write(ran.stderr)
            sys.stderr.write(f"the core did not produce a frame (exit {ran.returncode})\n")
            return 1
        width, height, pixels = load_ppm(shot)
        colours, runs = edge_colours(width, height, pixels)
        shown = ", ".join(f"#{colour.hex()}" for colour in colours[:12])
        if len(colours) != 2 or len(runs) != 2:
            sys.stderr.write(
                f"{component['id']} resampled a hard edge: {width}x{height}, "
                f"{len(colours)} colours in {len(runs)} runs ({shown}).\n"
                "The picture an export ships is the core's declared defaults "
                "plus the component's pixels entries.\n"
            )
            return 1
    print(f"{component['id']}: hard edge, {width}x{height}, two colours")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
