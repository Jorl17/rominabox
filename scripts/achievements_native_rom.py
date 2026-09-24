"""Original Mega Drive fixture with one observable work-RAM transition."""

import struct

from make_test_rom import make_megadrive_rom


def make_achievement_rom() -> bytes:
    rom = bytearray(make_megadrive_rom())
    # Replace the original terminal BRA with a bounded busy loop, followed by
    # a write to the first byte of work RAM ($FF0000). In rcheevos, with the
    # word-swapped RAM of the core, this is achievement address one.
    end = rom.rfind(b"\x60\xfe")
    if end < 0:
        raise ValueError("diagnostic cartridge has no terminal loop")
    words = (
        0x13FC, 0, 0x00FF, 0,       # MOVE.B #0,($FF0000).L
        0x203C, 0x0007, 0xA120,     # MOVE.L #500000,D0
        0x5380, 0x66FC,             # SUBQ.L #1,D0; BNE to SUBQ
        0x13FC, 1, 0x00FF, 0,       # MOVE.B #1,($FF0000).L
        0x60FE,                     # BRA to itself
    )
    code = struct.pack(f">{len(words)}H", *words)
    rom[end:end + len(code)] = code
    struct.pack_into(">H", rom, 0x18E,
                     sum(struct.unpack(f">{(len(rom) - 0x200) // 2}H", rom[0x200:])) & 0xFFFF)
    return bytes(rom)
