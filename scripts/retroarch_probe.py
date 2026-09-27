"""Compile a test program against the fork's own RetroArch sources.

In a probe we call a few RetroArch functions and read a few of its tables, so
we test RetroArch itself and not a description of it. No part of RetroArch
starts, but the files of those functions refer to much more of RetroArch.
With the macOS linker and the GNU linker for ELF, uncalled code is dropped
before names are resolved, and with MinGW every name is resolved first. So
we link native_runtime/retroarch_unreached.c into every probe. It has a weak
stand-in for every other name, which a definition in a linked source
overrides, and reaching a stand-in stops the program with the stand-in's name.

    python3 scripts/retroarch_probe.py OUTPUT PROGRAM.c SOURCE...

We compile PROGRAM.c with the SOURCEs, named relative to vendor/retroarch,
into the directory OUTPUT, and print the program's path.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
FORK = ROOT / "vendor/retroarch"
UNREACHED = ROOT / "scripts/native_runtime/retroarch_unreached.c"
# Drop uncalled code before resolving names, with the linkers that can do it.
UNUSED = {
    "darwin": ["-Wl,-dead_strip"],
    "linux": ["-Wl,--gc-sections"],
    "win32": ["-Wl,--gc-sections"],
}


def build(programs: list[Path], sources: list[str], output: Path, defines: list[str] | None = None) -> Path:
    """Compile `programs`, the first of which contains main, with the fork's
    `sources` and the stand-ins. Return the program, named after the first."""
    toolchain.activate()
    cc = toolchain.describe()["cc"]
    output.mkdir(parents=True, exist_ok=True)
    # input_driver.h includes "../config.h", a file written by RetroArch's
    # configure script. The code we probe does not depend on it, so we add an
    # empty one, one directory above an include path of its own.
    configured = output / "configured"
    (configured / "include").mkdir(parents=True, exist_ok=True)
    (configured / "config.h").write_text("", encoding="utf-8")
    includes = [f"-I{configured / 'include'}", f"-I{FORK}",
                f"-I{FORK / 'libretro-common/include'}", f"-I{FORK / 'deps'}"]
    forked = [FORK / source for source in sources]
    compiled = [*forked, *programs, UNREACHED]
    stems = [source.stem for source in compiled]
    if len(set(stems)) != len(stems):
        raise SystemExit(f"two sources would compile to one object: {sorted(stems)}")
    objects = []
    for source in compiled:
        # We compile the fork's sources as they are, and ours without warnings.
        warnings = ["-w"] if source in forked else ["-Wall", "-Werror"]
        built = output / f"{source.stem}.o"
        subprocess.run([cc, "-std=gnu99", "-ffunction-sections", "-fdata-sections", *warnings,
                        *(defines or []), *includes, "-c", str(source), "-o", str(built)], check=True)
        objects.append(str(built))
    binary = toolchain.executable(output / programs[0].stem)
    subprocess.run([cc, *UNUSED[sys.platform], *objects, "-o", str(binary)], check=True)
    return binary


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    output, program, *sources = sys.argv[1:]
    print(build([Path(program)], sources, Path(output)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
