"""Check the reading of launcher variables in the player, on every platform.

In the fork we read the ROM-in-a-Box variables through one reader,
vendor/retroarch/rominabox_environment.h. It returns the wide environment as
UTF-8 on Windows and the bytes elsewhere. We treat a variable as set whatever
its value, including an empty one, because an empty ROMINABOX_MENU_SCRIPT is
the script that takes a picture of the menu as it opens. Here we compile the
reader into a small program and start it with each case in its environment,
in the same way as we start the player from the launcher.

    uv run python scripts/test_environment_reader.py
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import scratch  # noqa: E402
import toolchain  # noqa: E402

FORK = ROOT / "vendor/retroarch"
CASES = {
    "RIB_READER_VALUE": "value",
    "RIB_READER_EMPTY": "",
    # A folder with a non-ASCII name, outside any home folder. We only read the value back.
    "RIB_READER_PATH": str(Path("C:/Games/João") if os.name == "nt" else Path("/opt/Games/João")),
}
UNSET = "RIB_READER_UNSET"


def main() -> int:
    toolchain.activate()
    declared = toolchain.describe()
    with scratch.scratch("rominabox-environment-reader-") as made:
        probe = toolchain.executable(Path(made) / "environment-probe")
        subprocess.run(
            [declared["cc"], "-std=gnu99", "-Wall", "-Werror",
             "-I", str(FORK), "-I", str(FORK / "libretro-common/include"),
             "-o", str(probe),
             str(ROOT / "scripts/native_runtime/environment_probe.c"),
             str(FORK / "libretro-common/encodings/encoding_utf.c"),
             str(FORK / "libretro-common/compat/compat_strl.c")],
            check=True,
        )
        environment = {name: value for name, value in os.environ.items() if name != UNSET}
        environment.update(CASES)
        read = subprocess.run([str(probe), *CASES, UNSET], env=environment, capture_output=True, check=True)
    lines = read.stdout.decode("utf-8").splitlines()
    expected = [f"{name}=[{value}]" for name, value in CASES.items()] + [f"{UNSET}=<unset>"]
    failures = 0
    for index, want in enumerate(expected):
        got = lines[index] if index < len(lines) else None
        if got == want:
            print(f"  ok   {want}")
        else:
            print(f"  FAIL read {got!r}, not {want!r}")
            failures += 1
    if failures:
        return 1
    print("the player reads a set, an empty, a non-ASCII and an unset variable as its launcher left them")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
