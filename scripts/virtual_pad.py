"""A virtual Xbox 360 pad that we drive in a Windows test, on the ViGEm bus
driver (scripts/native_runtime/windows/virtual_pad.c).

    with virtual_pad.plugged() as pad:
        pad.press("start")
        pad.rumble(2000)      # (large, small), 0-255 each, or None

The pad exists only inside the `with`, and Windows has no controller before
or after it. `slots()` returns the XInput slots, of four, with a pad, so
that in a test we can show that nothing is plugged in outside the test.
"""

from __future__ import annotations

import ctypes
import subprocess
import sys
import time
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import toolchain  # noqa: E402

SOURCE = ROOT / "scripts/native_runtime/windows/virtual_pad.c"
OUTPUT = ROOT / "work/test-output/virtual-pad"
# How long we keep a button down: longer than a frame at any game frame rate.
PRESS_SECONDS = 0.2


class NoBus(RuntimeError):
    """The ViGEm bus driver is not installed on this computer."""


def build() -> Path:
    """Return the tool, which we compile again when its source is newer."""
    toolchain.activate()
    binary = toolchain.executable(OUTPUT / "virtual-pad")
    if not binary.is_file() or binary.stat().st_mtime < SOURCE.stat().st_mtime:
        OUTPUT.mkdir(parents=True, exist_ok=True)
        subprocess.run([toolchain.describe()["cc"], "-std=gnu99", "-O1", "-Wall", "-Wextra", "-Werror",
                        str(SOURCE), "-o", str(binary), "-lsetupapi"], check=True)
    return binary


class _XInputState(ctypes.Structure):
    _fields_ = [("packet", ctypes.c_uint32), ("buttons", ctypes.c_uint16), ("left_trigger", ctypes.c_ubyte),
                ("right_trigger", ctypes.c_ubyte), ("left_x", ctypes.c_short), ("left_y", ctypes.c_short),
                ("right_x", ctypes.c_short), ("right_y", ctypes.c_short)]


def slots() -> list[int]:
    """Return the XInput slots with a pad, physical or virtual, as in a game."""
    xinput = ctypes.WinDLL("xinput1_4")
    held = []
    for slot in range(4):
        state = _XInputState()
        if xinput.XInputGetState(slot, ctypes.byref(state)) == 0:
            held.append(slot)
    return held


class Pad:
    def __init__(self, process: subprocess.Popen) -> None:
        self._process = process
        self.slot: int | None = None

    def _ask(self, command: str) -> str:
        self._process.stdin.write(command + "\n")
        self._process.stdin.flush()
        answer = self._process.stdout.readline().strip()
        if answer == "no-bus":
            raise NoBus("the ViGEm bus driver is not installed")
        if not answer or answer.startswith("failed"):
            raise RuntimeError(f"virtual pad: {command!r} answered {answer!r}")
        return answer

    def hold(self, *buttons: str) -> None:
        self._ask(" ".join(("hold", *buttons)))

    def press(self, *buttons: str, seconds: float = PRESS_SECONDS) -> None:
        self.hold(*buttons)
        time.sleep(seconds)
        self.hold()
        time.sleep(seconds)

    def rumble(self, milliseconds: int) -> tuple[int, int] | None:
        """Return the next motor setting within `milliseconds` as (large, small),
        with (0, 0) for a stop. The first value can be the previous setting."""
        answer = self._ask(f"rumble {milliseconds}").split()
        return None if answer[1] == "none" else (int(answer[1]), int(answer[2]))


@contextmanager
def plugged() -> Iterator[Pad]:
    """A virtual Xbox 360 pad, plugged in for the `with` and unplugged after,
    whatever happens inside it."""
    process = subprocess.Popen([str(build())], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    pad = Pad(process)
    try:
        pad.slot = int(pad._ask("plug").split()[1])
        yield pad
    finally:
        try:
            if pad.slot is not None:
                pad._ask("unplug")
        finally:
            process.stdin.close()
            process.wait(timeout=10)
