"""Generate an original animated Game Boy diagnostic cartridge, with no downloaded game content."""

from enum import IntEnum
from pathlib import Path


class GameBoyCartridge(IntEnum):
    """A Game Boy cartridge's type, as its header declares it at 0x147."""

    ROM_ONLY = 0x00
    # MBC5 with a rumble motor, as in Pokemon Pinball. Bit 3 of a write to
    # 0x4000-0x4FFF turns on the motor, and then player 1's pad rumbles.
    MBC5_RUMBLE = 0x1C


def make_megadrive_rom() -> bytes:
    """Original 32 KiB diagnostic: initialise the VDP and show a red field.

    For menu workflows we require a console with two controllers here.
    This is original 68000 code, with no game data or SDK linked into it.
    Hardware register reference: https://stephane-d.github.io/SGDK/vdp_8h.html
    """
    import struct

    rom = bytearray(32768)
    struct.pack_into(">II", rom, 0, 0x00FFFF00, 0x200)
    rom[0x100:0x110] = b"SEGA MEGA DRIVE "
    rom[0x120:0x150] = b"ROM-IN-A-BOX MENU TEST".ljust(48)
    rom[0x150:0x180] = b"ROM-IN-A-BOX MENU TEST".ljust(48)
    rom[0x190:0x1A0] = b"J".ljust(16)
    struct.pack_into(">IIII", rom, 0x1A0, 0, len(rom) - 1, 0xFF0000, 0xFFFFFF)
    rom[0x1F0:0x200] = b"U".ljust(16)
    words = [0x46FC, 0x2700]  # Disable interrupts; the fixture polls no input.

    def write_word(address: int, value: int) -> None:
        words.extend((0x33FC, value, address >> 16, address & 0xFFFF))

    def write_long(address: int, value: int) -> None:
        words.extend((0x23FC, value >> 16, value & 0xFFFF, address >> 16, address & 0xFFFF))

    write_word(0xA11100, 0x0100)  # Hold the unused sound CPU.
    write_long(0xA14000, 0x53454741)  # Hardware security signature, "SEGA".
    control, data = 0xC00004, 0xC00000
    registers = (0x04, 0x04, 0x30, 0x3C, 0x07, 0x6C, 0, 0, 0, 0, 0, 0, 0x81, 0x3F, 0, 2, 0, 0, 0)
    for index, value in enumerate(registers):
        write_word(control, 0x8000 | index << 8 | value)
    write_long(control, 0x40000000)  # VRAM write, address zero.
    words.extend((0x303C, 0x7FFF))  # 32,768 zero words clear VRAM.
    write_word(data, 0)
    words.extend((0x51C8, 0xFFF6))  # DBRA to the write above.
    write_long(control, 0x40000010)  # VSRAM write, zero vertical scroll.
    write_word(data, 0)
    write_word(data, 0)
    write_long(control, 0xC0000000)  # CRAM colour zero, a saturated red.
    write_word(data, 0x000E)
    write_word(control, 0x8144)  # Enable display after initialisation.
    words.append(0x60FE)  # BRA to itself; frames remain deterministic.
    rom[0x200:0x200 + len(words) * 2] = struct.pack(f">{len(words)}H", *words)
    checksum = sum(struct.unpack(f">{(len(rom) - 0x200) // 2}H", rom[0x200:])) & 0xFFFF
    struct.pack_into(">H", rom, 0x18E, checksum)
    return bytes(rom)


def make_rom(rumble: bool = False) -> bytes:
    """Create a 32 KiB cartridge showing moving colored stripes, whose colours
    swap while A is held. With `rumble`, a rumble cartridge whose motor also
    runs while A is held, and only then."""
    rom = bytearray(32768)
    rom[0x100:0x104] = bytes([0, 0xC3, 0x50, 0x01])
    # Fixed cartridge-header logo bytes required by the Game Boy boot protocol.
    rom[0x104:0x134] = bytes.fromhex(
        "CEED6666CC0D000B03730083000C000D0008111F8889000EDCCC6EE6DDDDD999BBBB67636E0EECCCDDDC999FBBB9333E"
    )
    rom[0x134:0x140] = b"ROM IN A BOX"
    rom[0x143] = 0x80
    rom[0x147] = GameBoyCartridge.MBC5_RUMBLE if rumble else GameBoyCartridge.ROM_ONLY
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
    if rumble:
        emit(0x06, 0x00)  # B=motor off; LD keeps BIT's Z
    jump(0x20, "palette")
    emit(0x3E, 0x1B)
    if rumble:
        emit(0x06, 0x08)  # A held: B=motor on, bit 3
    mark("palette")
    emit(0xE0, 0x47)
    if rumble:
        emit(0x78, 0xEA, 0x00, 0x40)  # the motor, once a frame: LD A,B; LD (4000),A
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
    parser.add_argument("--system", choices=("gbc", "megadrive"), default="gbc")
    parser.add_argument("--rumble", action="store_true", help="gbc: a rumble cartridge whose motor runs while A is held")
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(make_megadrive_rom() if args.system == "megadrive" else make_rom(args.rumble))
    print(args.output)
