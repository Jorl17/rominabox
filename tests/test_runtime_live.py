"""Opt-in headless integration with the pinned native runtime and original test content."""

import os
import struct
import subprocess
import zlib
from pathlib import Path

import pytest
from PIL import Image

from rominabox.domain import Project
from rominabox.runtime import RuntimeSession

pytestmark = [
    pytest.mark.runtime,
    pytest.mark.skipif(
        os.environ.get("ROMINABOX_RUNTIME_TEST") != "1",
        reason="Runtime kit required; opt in to headless emulator checks",
    ),
]


def state_memory(data: bytes) -> tuple[bytearray, int, int]:
    """Locate Gambatte WRAM in this pinned core's tagged state format inside RetroArch RZIP."""
    assert data[:8] == b"#RZIPv\x01#"
    raw = bytearray(zlib.decompress(data[24:]))
    assert raw[:8] == b"RASTATE\x01"
    assert raw[8:12] == b"MEM "
    end = 16 + int.from_bytes(raw[12:16], "little")
    offset = 21  # RetroArch header, Gambatte version and empty embedded snapshot.
    while offset < end:
        zero = raw.index(0, offset)
        label = raw[offset:zero]
        size = int.from_bytes(raw[zero + 1 : zero + 4], "big")
        start = zero + 4
        if label == b"wram":
            return raw, start, size
        offset = start + size
    raise AssertionError("Missing WRAM block")


def with_memory_marker(data: bytes) -> bytes:
    """Seed unused cartridge RAM so a restart cannot pass as a successful restore."""
    raw, offset, _ = state_memory(data)
    raw[offset + 1024 : offset + 1040] = b"ROMINABOX-STATE!"
    compressed = zlib.compress(raw)
    return data[:20] + struct.pack("<I", len(compressed)) + compressed


def memory_marker(data: bytes) -> bytes:
    raw, offset, _ = state_memory(data)
    return bytes(raw[offset + 1024 : offset + 1040])


def test_checkpoint_restore_with_real_gambatte(tmp_path):
    root = Path(__file__).resolve().parents[1]
    rom = tmp_path / "game.gbc"
    subprocess.run([str(root / ".venv/bin/python"), str(root / "scripts/make_test_rom.py"), str(rom)], check=True)
    project = Project.from_rom(rom)
    session = RuntimeSession(project, root / "work/runtime-kit", tmp_path / "User data with spaces é")

    def play(resume):
        session.start(resume, max_frames=90, headless=True)
        try:
            session.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            session.process.kill()
            session.process.wait()
            pytest.fail("RetroArch did not exit at its frame limit")
        assert session.finish(), (session.home / "logs/retroarch.log").read_text()[-5000:]
        with Image.open(session.store.checkpoint.with_name("game.state.auto.png")) as preview:
            assert preview.size == (160, 144)
            assert any(low != high for low, high in preview.getextrema()), (
                "Diagnostic cartridge should render visible stripes"
            )
        return session.store.checkpoint.read_bytes()

    initial = play(False)
    marker = b"ROMINABOX-STATE!"
    assert memory_marker(initial) != marker
    seeded = with_memory_marker(initial)
    session.store.checkpoint.write_bytes(seeded)
    session.store.save(1)
    second = play(True)
    assert memory_marker(second) == marker, "Resume must restore actual cartridge RAM, not restart the ROM"
    fresh = play(False)
    assert memory_marker(fresh) != marker, "Starting fresh must not restore the previous checkpoint"
    session.store.restore(1)
    assert session.store.checkpoint.read_bytes() == seeded
    restored = play(True)
    assert memory_marker(restored) == marker, "A manual slot must restore cartridge RAM after a fresh session"
