"""Build a headless test program from the menu's code, one cached object at a time.

We list the menu's sources once, in the HAVE_RMLUI block of
vendor/retroarch/Makefile.common. We read that block here, so a new menu file
is added only there and we compile it into every harness.

    uv run python scripts/native_runtime/menu_harness.py build OUT [--define NAME]... [--framework NAME]... SOURCE...
    uv run python scripts/native_runtime/menu_harness.py sources

With `build` we compile the menu into an archive and link OUT from the given
test sources, that archive, libretro's file layer (through which we read the
menu's files), RmlUi and FreeType. Only the parts of the archive that the
program uses are linked, so a probe of the account form does not require a
fake for every host command. With `sources` we print the list from the block.

The object cache is in work/menu-harness/, with one directory per compiler
and flag set. We reuse an object while its source, every header listed for it
by -MMD and the flags are unchanged, and we relink the program when any
object, library or link flag changes. We compare contents and not times, so
we rebuild nothing after a checkout switch in which files are touched but
not changed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import file_lock  # noqa: E402
import native_build  # noqa: E402
import rmlui_paths  # noqa: E402
import toolchain as declared_toolchain  # noqa: E402
RETROARCH = ROOT / "vendor/retroarch"
MAKEFILE = RETROARCH / "Makefile.common"
BRIDGE = RETROARCH / "menu/drivers"
LIBRETRO_INCLUDE = RETROARCH / "libretro-common/include"
CACHE = ROOT / "work/menu-harness"

# libretro's file layer, through which we read and write the menu's files, as
# declared in the player recipe. We link it into every harness program.
FILE_LAYER = [RETROARCH / name for name in native_build.recipe()["fileLayer"]["sources"]]

# Objects in the block that we leave out of a headless harness, and why.
HEADLESS_EXCLUDED = {
    "menu/drivers/rmlui/driver.o": "RetroArch's menu driver table; a harness calls the menu API itself",
    "menu/drivers/rmlui/host.o": "the live RetroArch host; each harness links a fake one",
    "menu/drivers/rmlui/video.o": "the live RetroArch shaders, for the brightness and contrast pass, for which we answer in the fake host",
    "menu/drivers/rmlui/pad_inputs.o": "reads RetroArch's pads through the live host; the fake host answers for it",
    "menu/drivers/rmlui/text_host.o": "RetroArch's keyboard; text_test_host.cpp stands in",
    "menu/drivers/rmlui/render/rmlui_gl.o": "draws with OpenGL; headless has no context",
    "menu/drivers/rmlui/render/rmlui_gl3.o": "draws with OpenGL; headless has no context",
    "menu/drivers/third_party/lodepng.o": "used only by code RIB_RMLUI_HEADLESS compiles out",
}

# Our answer in a headless build to each condition nested in the block. For a
# condition missing from this table we stop the build instead of guessing.
HEADLESS_CONDITIONS = {
    # No platform text service, because text_input_macos.mm is included in
    # the composition probe itself, and there is no Windows window in a harness.
    "ifeq ($(HAVE_COCOA), 1)": False,
    "ifneq ($(findstring Win32,$(OS)),)": False,
    # We link no harness with rcheevos, so we build the menu with the stub
    # used in a player without achievements.
    "ifneq ($(HAVE_CHEEVOS), 1)": True,
}

SOURCE_SUFFIXES = (".c", ".cpp", ".mm", ".m")
_OPENING = ("ifeq", "ifneq", "ifdef", "ifndef")


def menu_sources(makefile: Path = MAKEFILE, base: Path = RETROARCH,
                 excluded: dict[str, str] = HEADLESS_EXCLUDED) -> list[Path]:
    """Return the menu's sources in the player build, minus those we leave out headless."""
    if not makefile.is_file():
        raise SystemExit(f"missing {makefile}")
    lines = makefile.read_text(encoding="utf-8", errors="replace").replace("\\\n", " ").splitlines()
    start = next((index for index, line in enumerate(lines)
                  if line.strip() == "ifeq ($(HAVE_RMLUI), 1)"), None)
    if start is None:
        raise SystemExit(f"{makefile.name} has no `ifeq ($(HAVE_RMLUI), 1)` block to read the menu sources from")
    objects: list[str] = []
    # One entry per open condition inside the block: whether we take it headless.
    taken: list[bool] = []
    for line in lines[start + 1:]:
        words = line.split()
        if not words:
            continue
        if words[0] in _OPENING:
            condition = " ".join(words)
            if condition not in HEADLESS_CONDITIONS:
                raise SystemExit(
                    f"{makefile.name}'s HAVE_RMLUI block has `{condition}`; "
                    "say in HEADLESS_CONDITIONS whether a headless build takes it")
            taken.append(HEADLESS_CONDITIONS[condition])
        elif words[0] == "else":
            if not taken:
                break
            if len(words) > 1:
                raise SystemExit(f"{makefile.name}'s HAVE_RMLUI block has `{line.strip()}`, which this does not read")
            taken[-1] = not taken[-1]
        elif words[0] == "endif":
            if not taken:
                break
            taken.pop()
        elif len(words) > 2 and words[0] == "OBJ" and words[1] == "+=" and all(taken):
            objects.extend(words[2:])
    else:
        raise SystemExit(f"{makefile.name}'s HAVE_RMLUI block never ends")
    if not objects:
        raise SystemExit(f"{makefile.name}'s HAVE_RMLUI block adds no objects")
    stale = sorted(set(excluded) - set(objects))
    if stale:
        raise SystemExit(f"a headless exclusion names objects the block no longer has: {', '.join(stale)}")
    sources = []
    for name in objects:
        if not name.endswith(".o"):
            raise SystemExit(f"{makefile.name}'s HAVE_RMLUI block adds {name!r}, which is not an object")
        if name in excluded:
            continue
        found = [base / (name[:-2] + suffix) for suffix in SOURCE_SUFFIXES
                 if (base / (name[:-2] + suffix)).is_file()]
        if len(found) != 1:
            raise SystemExit(f"{name} in {makefile.name} should have one source; found {len(found)}")
        sources.append(found[0])
    return sources


_memo_lock = threading.Lock()


def _hash_file(path: Path, memo: dict[Path, str]) -> str | None:
    """Return the file's content hash, computed once per build, with `memo` shared by compile threads."""
    with _memo_lock:
        if path in memo:
            return memo[path]
    try:
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError:
        digest = None
    with _memo_lock:
        return memo.setdefault(path, digest)


def _digest(value) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def _depfile_inputs(text: str) -> list[str]:
    # The target ends at the first colon followed by whitespace. There is no
    # whitespace after a Windows drive letter ("C:\\").
    body = re.split(r":(?=\s|$)", text.replace("\\\n", " "), maxsplit=1)[1]
    return [token.replace("\\ ", " ") for token in re.findall(r"(?:\\ |\S)+", body)]


@dataclass(frozen=True)
class Toolchain:
    cc: str
    cxx: str
    cflags: tuple[str, ...]
    cxxflags: tuple[str, ...]

    def identity(self) -> dict:
        """Return the compiler versions, so we rebuild everything after an upgrade."""
        found = {}
        for tool in {self.cc, self.cxx}:
            resolved = shutil.which(tool)
            if not resolved:
                raise SystemExit(f"no {tool} on PATH")
            version = subprocess.run([resolved, "--version"], capture_output=True, text=True, check=True)
            found[tool] = [os.path.realpath(resolved), version.stdout]
        # System headers are missing from the -MMD output, so we must rebuild
        # after an SDK update even when the compiler version is the same.
        if sys.platform == "darwin":
            found["sdk"] = [subprocess.run(["xcrun", "--sdk", "macosx", *query], capture_output=True,
                                           text=True, check=True).stdout
                            for query in (["--show-sdk-path"], ["--show-sdk-version"])]
        return found

    def command(self, source: Path) -> list[str]:
        if source.suffix == ".c":
            return [self.cc, *self.cflags]
        return [self.cxx, *self.cxxflags]


def _object_name(source: Path) -> str:
    try:
        relative = source.relative_to(ROOT)
    except ValueError:
        relative = Path(*source.parts[1:])
    return "__".join(relative.parts) + ".o"


def compile_objects(sources: list[Path], toolchain: Toolchain, directory: Path,
                    identity: dict | None = None) -> tuple[list[Path], list[Path]]:
    """Return objects for `sources` in `directory`, and which sources we compiled."""
    directory.mkdir(parents=True, exist_ok=True)
    identity = identity if identity is not None else toolchain.identity()
    memo: dict[Path, str] = {}
    objects, stale = [], []
    for source in sources:
        target = directory / _object_name(source)
        key = _digest([identity, toolchain.command(source), str(source)])
        objects.append(target)
        try:
            recorded = json.loads(target.with_suffix(".json").read_text())
        except (OSError, ValueError):
            recorded = None
        if (not target.is_file() or not recorded or recorded.get("key") != key
                or any(_hash_file(Path(path), memo) != digest
                       for path, digest in recorded.get("inputs", {}).items())):
            stale.append((source, target, key))

    def build(entry: tuple[Path, Path, str]) -> None:
        source, target, key = entry
        partial = target.with_suffix(".partial.o")
        depfile = target.with_suffix(".partial.d")
        command = [*toolchain.command(source), "-MMD", "-MF", str(depfile),
                   "-c", str(source), "-o", str(partial)]
        compiled = subprocess.run(command, capture_output=True, text=True)
        if compiled.returncode != 0:
            raise SystemExit(f"{shlex.join(command)}\n{compiled.stdout}{compiled.stderr}")
        if compiled.stderr:
            print(compiled.stderr, end="", file=sys.stderr)
        inputs = {path: _hash_file(Path(path), memo) for path in _depfile_inputs(depfile.read_text())}
        depfile.unlink()
        # Write the record last, so that after a build stopped in between, the
        # record does not match the object and we rebuild it in the next run.
        os.replace(partial, target)
        target.with_suffix(".json").write_text(json.dumps({"key": key, "inputs": inputs}, indent=1))

    with ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as pool:
        list(pool.map(build, stale))
    return objects, [source for source, _, _ in stale]


def _stamped(output: Path, inputs: list[Path], command: list[str]) -> tuple[bool, Path, str]:
    """Whether `output` was made from exactly these files by this command."""
    stamp = output.with_name(output.name + ".inputs.json")
    memo: dict[Path, str] = {}
    digest = _digest([command, [[str(path), _hash_file(path, memo)] for path in inputs]])
    try:
        current = output.is_file() and json.loads(stamp.read_text()).get("digest") == digest
    except (OSError, ValueError):
        current = False
    return current, stamp, digest


def archive(objects: list[Path], output: Path) -> bool:
    """Pack `objects` into `output`, or return False when it already contains exactly them."""
    command = ["ar", "rcs"]
    current, stamp, digest = _stamped(output, objects, command)
    if current:
        return False
    partial = output.with_name(output.name + ".partial")
    if partial.is_file():
        partial.unlink()
    subprocess.run([*command, str(partial), *map(str, objects)], check=True)
    os.replace(partial, output)
    stamp.write_text(json.dumps({"digest": digest}))
    return True


def link(output: Path, inputs: list[Path], toolchain: Toolchain, flags: list[str]) -> bool:
    """Link `output` from object and archive `inputs`, or return False when nothing changed."""
    command = [toolchain.cxx, *flags]
    current, stamp, digest = _stamped(output, inputs, command)
    if current:
        return False
    output.parent.mkdir(parents=True, exist_ok=True)
    partial = output.with_name(output.name + ".partial")
    linked = subprocess.run([toolchain.cxx, "-o", str(partial), *map(str, inputs), *flags],
                            capture_output=True, text=True)
    if linked.returncode != 0:
        raise SystemExit(f"linking {output.name} failed\n{linked.stdout}{linked.stderr}")
    os.replace(partial, output)
    stamp.write_text(json.dumps({"digest": digest}))
    return True


def headless(defines: list[str]) -> Toolchain:
    """Return the flags for compiling the menu in every harness, plus the program's own defines."""
    freetype = rmlui_paths.freetype("--cflags")
    return Toolchain(
        cc="cc",
        cxx="c++",
        cflags=("-I", str(LIBRETRO_INCLUDE)),
        cxxflags=("-std=c++17", "-Werror=return-type", "-DRIB_RMLUI_HEADLESS",
                  *(f"-D{name}" for name in defines),
                  *rmlui_paths.DEFINES,
                  *(f"-I{path}" for path in rmlui_paths.HEADER_DIRS),
                  "-I", str(BRIDGE), "-I", str(LIBRETRO_INCLUDE), *freetype),
    )


def build(output: Path, sources: list[Path], defines: list[str], frameworks: list[str]) -> str:
    if not rmlui_paths.LIBRARY.is_file():
        raise SystemExit(f"missing {rmlui_paths.LIBRARY} (uv run python scripts/prepare_rmlui.py)")
    started = time.monotonic()
    toolchain = headless(defines)
    identity = toolchain.identity()
    variant = CACHE / _digest([identity, toolchain.cflags, toolchain.cxxflags])[:16]
    variant.mkdir(parents=True, exist_ok=True)
    (variant / "flags.json").write_text(json.dumps(
        {"cflags": toolchain.cflags, "cxxflags": toolchain.cxxflags}, indent=1))
    freetype = rmlui_paths.freetype("--libs")
    menu = menu_sources()
    # We read the menu's files through libretro's file layer, so we link
    # it into every program with the menu.
    sources = [*sources, *(source for source in FILE_LAYER if source not in sources)]
    # When two harness builds run at once, the second waits until the first is done.
    with open(variant / "lock", "w") as lock:
        file_lock.hold_exclusively(lock)
        menu_objects, menu_compiled = compile_objects(menu, toolchain, variant / "objects", identity)
        program_objects, program_compiled = compile_objects(sources, toolchain, variant / "objects", identity)
        archived = archive(menu_objects, variant / "libmenu.a")
        linked = link(output, [*program_objects, variant / "libmenu.a", rmlui_paths.LIBRARY], toolchain,
                      [*freetype, *(flag for name in frameworks for flag in ("-framework", name)),
                       *declared_toolchain.describe()["supportLibraries"]])
    compiled = len(menu_compiled) + len(program_compiled)
    return (f"menu harness: {output.name}: compiled {compiled} of {len(menu) + len(sources)} objects, "
            f"archive {'rebuilt' if archived else 'reused'}, {'linked' if linked else 'link reused'}, "
            f"{time.monotonic() - started:.1f}s")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    making = commands.add_parser("build", help="link a test program against the menu")
    making.add_argument("output", type=Path)
    making.add_argument("sources", nargs="+", type=Path)
    making.add_argument("--define", action="append", default=[])
    making.add_argument("--framework", action="append", default=[])
    commands.add_parser("sources", help="print the menu sources a harness compiles")
    arguments = parser.parse_args()
    if arguments.command == "sources":
        for source in menu_sources():
            print(source.relative_to(ROOT))
        return 0
    sources = arguments.sources
    missing = [str(source) for source in sources if not source.is_file()]
    if missing:
        raise SystemExit(f"no such source: {', '.join(missing)}")
    print(build(arguments.output.resolve(), [source.resolve() for source in sources],
                arguments.define, arguments.framework), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
