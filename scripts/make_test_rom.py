"""Generate an original animated Game Boy diagnostic cartridge, with no downloaded game content."""

from pathlib import Path


def make_rom() -> bytes:
    """Create a 32 KiB cartridge showing moving colored stripes."""
    rom = bytearray(32768)
    rom[0x100:0x104] = bytes([0, 0xC3, 0x50, 0x01])
    # Fixed cartridge-header logo bytes required by the Game Boy boot protocol.
    rom[0x104:0x134] = bytes.fromhex(
        "CEED6666CC0D000B03730083000C000D0008111F8889000EDCCC6EE6DDDDD999BBBB67636E0EECCCDDDC999FBBB9333E"
    )
    rom[0x134:0x140] = b"ROM IN A BOX"
    rom[0x143] = 0x80
    code = bytearray()
    labels: dict[str, int] = {}
    jumps: list[tuple[int, str]] = []

    def emit(*values: int) -> None:
        code.extend(values)

    def mark(name: str) -> None:
        labels[name] = len(code)

    def jump(op: int, name: str) -> None:
        emit(op, 0)
        jumps.append((len(code) - 1, name))

    emit(0xF3, 0x31, 0xFE, 0xFF)  # DI; SP=FFFE
    mark("wait_boot")
    emit(0xF0, 0x44, 0xFE, 144)  # wait for VBlank before disabling LCD
    jump(0x38, "wait_boot")
    emit(0xAF, 0xE0, 0x40, 0xE0, 0x42, 0xE0, 0x43)
    emit(0x3E, 0x80, 0xE0, 0x68)  # CGB background palette, auto-increment
    for component in (0xFF, 0x7F, 0xFF, 0x03, 0x1F, 0x7C, 0x00, 0x00):
        emit(0x3E, component, 0xE0, 0x69)
    emit(0x21, 0x00, 0x80, 0x06, 8)  # HL=tile0, B=8 lines
    mark("tile")
    emit(0x3E, 0xAA, 0x22, 0x3E, 0xCC, 0x22, 0x05)
    jump(0x20, "tile")
    emit(0x21, 0x00, 0x98, 0x01, 0x00, 0x04)
    mark("map")
    emit(0xAF, 0x22, 0x0B, 0x78, 0xB1)
    jump(0x20, "map")
    emit(0x3E, 0xE4, 0xE0, 0x47, 0x3E, 0x91, 0xE0, 0x40)
    mark("frame")
    emit(0xF0, 0x44, 0xFE, 144)
    jump(0x38, "frame")
    emit(0xF0, 0x43, 0x3C, 0xE0, 0x43)  # scroll once per frame
    emit(0x3E, 0x10, 0xE0, 0x00, 0xF0, 0x00, 0xCB, 0x47)
    emit(0x3E, 0xE4)
    jump(0x20, "palette")
    emit(0x3E, 0x1B)
    mark("palette")
    emit(0xE0, 0x47)
    mark("end_vblank")
    emit(0xF0, 0x44, 0xFE, 144)
    jump(0x30, "end_vblank")
    jump(0x18, "frame")
    for offset, name in jumps:
        relative = labels[name] - (offset + 1)
        assert -128 <= relative <= 127
        code[offset] = relative & 255
    rom[0x150 : 0x150 + len(code)] = code
    checksum = 0
    for value in rom[0x134:0x14D]:
        checksum = (checksum - value - 1) & 255
    rom[0x14D] = checksum
    total = sum(rom) & 65535
    rom[0x14E:0x150] = total.to_bytes(2, "big")
    return bytes(rom)


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(make_rom())
    print(args.output)
