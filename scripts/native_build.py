"""The player's native build: one recipe, and the steps common to all targets.

`scripts/native_runtime/player-recipe.json` lists what we build, and in this
module we build it, for the player (`scripts/build_player.py`) and for the
RmlUi linked into the tests (`scripts/prepare_rmlui.py`), so the two builds
cannot differ. Targets differ only where the recipe has a key for a platform
or a target or a function here is specific to one: the processor and
system version of a slice, the name of the finished binary, the spelling of
its symbols and the libraries we may link into it.
"""

from __future__ import annotations

import enum
import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import file_lock  # noqa: E402
import toolchain  # noqa: E402
from core_source import DOWNLOADS  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
RECIPE_PATH = ROOT / "scripts/native_runtime/player-recipe.json"
FORK = ROOT / "vendor/retroarch"


def recipe() -> dict:
    return json.loads(RECIPE_PATH.read_text(encoding="utf-8"))


def require_target(target: str) -> str:
    """`target`, if the recipe has a player for it as one slice."""
    if target not in recipe()["targets"]:
        raise SystemExit(f"the player recipe has no target {target}; it has {', '.join(recipe()['targets'])}")
    return target


def universal_targets() -> dict[str, list[str]]:
    """Each universal target declared in the recipe, with its slices."""
    return {name: joined for name, joined in recipe()["universal"].items() if name != "comment"}


def require_build_target(target: str) -> str:
    """`target`, if the recipe has a player for it: one slice, or several joined."""
    known = [*recipe()["targets"], *universal_targets()]
    if target not in known:
        raise SystemExit(f"the player recipe has no target {target}; it has {', '.join(known)}")
    return target


def slices(target: str) -> list[str]:
    """The targets a build of `target` is made of: a universal target's
    slices, or the target itself."""
    joined = universal_targets().get(require_build_target(target))
    return [require_target(name) for name in joined] if joined else [target]


def kit_target(target: str) -> str:
    """The build target of the kit we use for games on a builder for `target`:
    the universal target that includes the slice, where the recipe declares
    a kit for one, or the target itself."""
    kits = recipe()["kit"]
    for name, parts in universal_targets().items():
        if require_target(target) in parts and name in kits:
            return name
    return target


def kit_file(target: str, role: str) -> str:
    """Where the file with `role` (player, launcher) is in the kit we use for
    games on a builder for `target`."""
    return recipe()["kit"][kit_target(target)]["files"][role]["at"]


class Architecture(enum.Enum):
    """A processor of a slice, spelled as in Apple's -arch and lipo and in the
    fork's ARCH."""

    ARM64 = "arm64"
    X86_64 = "x86_64"


def architecture_of(target: str) -> Architecture:
    """The processor in the name of a slice target, after its platform."""
    return Architecture(require_target(target).split("-", 1)[1])


def deployment_target(target: str) -> str:
    """The oldest system version the platform's player is built for."""
    declared = recipe()["deploymentTarget"].get(platform_of(target))
    if declared is None:
        raise SystemExit(f"the player recipe declares no deployment target for {platform_of(target)}")
    return declared


def is_windows(target: str) -> bool:
    return target.startswith("windows-")


def is_macos(target: str) -> bool:
    return target.startswith("macos-")


def binary_name(target: str) -> str:
    """The name of the player in the RetroArch makefile on this target."""
    if is_windows(target):
        return "retroarch.exe"
    if is_macos(target):
        return "retroarch"
    raise SystemExit(f"no player binary name for {target}")


def symbol_prefix(target: str) -> str:
    """How a C function's name appears in the binary's symbol table."""
    if is_macos(target):
        return "_"
    if is_windows(target):
        return ""
    raise SystemExit(f"no symbol prefix for {target}")


def configure_flags(target: str) -> list[str]:
    configure = recipe()["configure"]
    return [*configure["common"], *configure[platform_of(require_target(target))]]


def configure_environment(target: str) -> dict[str, str]:
    """What we run configure with on `target`'s platform, besides the build environment."""
    return dict(recipe()["configure"]["environment"].get(platform_of(require_target(target)), {}))


def configure_makefile(target: str) -> str:
    """configure.mk, with which we run the fork's configure for `target` when
    configure, its qb scripts or this file are newer than its last run. We
    keep unchanged outputs of configure as they were, so we do not compile
    them again."""
    exports = [f"export {name} = {value}" for name, value in configure_environment(target).items()]
    return "\n".join([
        *exports,
        "configured: configure $(wildcard qb/*) configure.mk",
        "\t$(SHELL) ./configure " + shlex.join(configure_flags(target)).replace("$", "$$"),
        "\t@touch configured",
    ]) + "\n"


def makefile_local(target: str, settings: dict[str, str]) -> str:
    """The build's Makefile.local: the recipe's lines for `target`, then
    `settings`, the choices of this build, which override any assignment in
    the fork's makefiles, as on the make command line. Every object depends
    on this file, so after a change of settings we compile everything again."""
    makefile = recipe()["makefile"]
    return "\n".join([*makefile["common"], *makefile[platform_of(require_target(target))],
                      *(f"override {name} = {value}" for name, value in settings.items())]) + "\n"


def make_variables(target: str) -> dict[str, str]:
    """The settings for the fork's makefile to build a slice for its processor
    and system version. With its ARCH switch we add -arch to every compile and
    the link, and with MINVERFLAGS we set the version for that processor."""
    if is_macos(target):
        return {"ARCH": architecture_of(target).value,
                "MINVERFLAGS": f"-mmacosx-version-min={deployment_target(target)}"}
    if is_windows(target):
        return {}
    raise SystemExit(f"no make variables for {target}")


def compiler_flags(target: str) -> list[str]:
    """What we add to a compile or link run directly, outside a makefile or
    cmake, to build for `target`'s processor and system version."""
    if is_macos(target):
        return ["-arch", architecture_of(target).value, f"-mmacosx-version-min={deployment_target(target)}"]
    if is_windows(target):
        return []
    raise SystemExit(f"no compiler flags for {target}")


def cmake_flags(target: str) -> list[str]:
    """What we add to a cmake configuration to build for `target`'s processor and system version."""
    if is_macos(target):
        return [f"-DCMAKE_OSX_ARCHITECTURES={architecture_of(target).value}",
                f"-DCMAKE_OSX_DEPLOYMENT_TARGET={deployment_target(target)}"]
    if is_windows(target):
        return []
    raise SystemExit(f"no cmake flags for {target}")


def build_environment(target: str) -> dict[str, str]:
    """The environment in which we run configure, make, cmake and the compilers."""
    environment = dict(os.environ)
    if is_windows(target):
        root = toolchain.msys2_root()
        environment["MSYSTEM"] = "UCRT64"
        environment["PATH"] = os.pathsep.join(
            [str(root / "ucrt64" / "bin"), str(root / "usr" / "bin"), environment.get("PATH", "")]
        )
    elif is_macos(target):
        # Read by configure, cmake, the compilers and the linker alike.
        environment["MACOSX_DEPLOYMENT_TARGET"] = deployment_target(target)
    else:
        raise SystemExit(f"no build environment for {target}")
    return environment


def make_path(path: Path, target: str) -> str:
    """A path in the form for make on this target.

    In MSYS2 make, a colon in a target name is the rule separator, so we
    write a drive letter in the MSYS form, with C:/tools as /c/tools.
    """
    if is_windows(target):
        drive, rest = path.as_posix().split(":", 1)
        return f"/{drive.lower()}{rest}"
    if is_macos(target):
        return str(path)
    raise SystemExit(f"no make path form for {target}")


def resolve(program: str, environment: dict[str, str]) -> str:
    """The program found through the PATH of the environment.

    On Windows a program is looked up on the caller's PATH, not the child's,
    so with an unresolved name we would run another installation's cmake.
    """
    found = shutil.which(program, path=environment.get("PATH"))
    if found is None:
        raise SystemExit(f"{program} is not on the build's PATH")
    return found


def run(command: list[str], cwd: Path, environment: dict[str, str]) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run([resolve(command[0], environment), *command[1:]], cwd=cwd, env=environment, check=True)


@dataclass(frozen=True)
class Step:
    """One file we make with ninja: `output`, from `inputs`, by `command`. A step
    with `compiles` also has a ninja depfile with the headers read in it."""
    output: Path
    inputs: list[Path]
    command: list[str]
    compiles: bool = False


# How we run a command through ninja on each system: CreateProcess on Windows,
# with Windows quoting, and /bin/sh on macOS and Linux.
COMMAND_LINE = {"nt": subprocess.list2cmdline, "posix": shlex.join}


def compile_step(objects: Path, index: int, source: Path, compiler: list[str]) -> Step:
    """`source` compiled with `compiler` into the folder `objects`, numbered by
    `index` so that two sources with one name do not collide."""
    output = objects / f"{index:02d}-{source.stem}.o"
    return Step(output, [source], [*compiler, "-c", str(source), "-o", str(output)], compiles=True)


def linked(program: Path) -> Path:
    """The unstripped program from which we make a stripped `program`. We keep it,
    because when the link is up to date it still has its symbols, and we make
    the stripped copy again from it."""
    return program.with_name(f"{program.stem}-linked{program.suffix}")


def ninja(folder: Path, steps: list[Step], environment: dict[str, str]) -> None:
    """Bring the output of every step up to date with ninja, with its files in
    `folder`. We run a step again only when an input, a header read in it or
    its command line changed since the last run."""
    def escaped(path: Path) -> str:
        return str(path).replace("$", "$$").replace(" ", "$ ").replace(":", "$:")

    lines = ["rule step", "  command = $command", "rule compile", "  command = $command",
             "  depfile = $out.d", "  deps = gcc"]
    for step in steps:
        command = [resolve(step.command[0], environment), *step.command[1:]]
        if step.compiles:
            command += ["-MD", "-MF", f"{step.output}.d"]
        lines += [f"build {escaped(step.output)}: {'compile' if step.compiles else 'step'} "
                  + " ".join(escaped(path) for path in step.inputs),
                  "  command = " + COMMAND_LINE[os.name](command).replace("$", "$$")]
    folder.mkdir(parents=True, exist_ok=True)
    write_if_changed(folder / "build.ninja", ("\n".join(lines) + "\n").encode("utf-8"))
    run(["ninja", "-C", str(folder)], folder, environment)


def fork_commit() -> str:
    if subprocess.run(["git", "-C", str(FORK), "status", "--porcelain"],
                      capture_output=True, text=True, check=True).stdout.strip():
        raise SystemExit("commit native source changes before building")
    return subprocess.run(["git", "-C", str(FORK), "rev-parse", "HEAD"],
                          capture_output=True, text=True, check=True).stdout.strip()


def write_if_changed(path: Path, data: bytes) -> None:
    """`data` at `path`, written only when it differs from what is there, so
    that a file read in a build keeps its time and we do not rebuild what
    depends on it."""
    if path.is_file() and path.read_bytes() == data:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def checkout_fork(commit: str, destination: Path, target: str) -> None:
    """The committed fork at `commit` in destination/retroarch, with LF line
    endings whatever the checkout has. The Git data is next to the folder, in
    fork.git, with the fork's objects shared, and the folder contains only the
    fork's files, because in the fork's makefile we build a version from any
    .git inside it. We move a folder from an earlier build to `commit` with
    git, rewriting only the files that differ and removing those not in
    `commit`, so we compile only what changed, and the build's files stay."""
    repository = destination / "fork.git"
    source = destination / "retroarch"
    if not repository.is_dir():
        subprocess.run(["git", "clone", "--quiet", "--bare", "--shared", str(FORK), str(repository)], check=True)
    source.mkdir(parents=True, exist_ok=True)
    settings = ["core.bare=false", "core.autocrlf=false", "advice.detachedHead=false",
                *recipe()["checkout"][platform_of(require_target(target))]]
    subprocess.run(["git", f"--git-dir={repository}", f"--work-tree={source}",
                    *(part for setting in settings for part in ("-c", setting)),
                    "checkout", "--quiet", "--force", "--detach", commit], check=True)
    forget_removed_sources(source)


def forget_removed_sources(source: Path) -> None:
    """Remove each object from an earlier build in `source` whose source is no
    longer in the fork, with its dependency file, so that we compile it again
    from its current source. The source of the object is first in a dependency
    file, so for example after upstream rewrote shader_gl3.cpp as shader_gl3.c,
    the object kept its name and the build stopped at the .cpp with "No rule to
    make target". A removed header requires nothing here, because in the fork's
    makefile each header has a separate rule (-MP)."""
    for dependencies in source.glob("obj-*/**/*.d"):
        rule = dependencies.read_text(encoding="utf-8", errors="replace").replace("\\\n", " ")
        named = rule.partition(":")[2].split()
        if named and not (source / named[0]).exists():
            dependencies.with_suffix(".o").unlink(missing_ok=True)
            dependencies.unlink()


def fetch_rmlui(destination: Path) -> Path:
    """RmlUi at the recipe's commit, cloned once into `destination`."""
    rmlui = recipe()["rmlui"]
    source = destination / rmlui["source"]
    if not (source / ".git").exists():
        source.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "clone", rmlui["repository"], str(source)], check=True)
    subprocess.run(["git", "-C", str(source), "-c", "advice.detachedHead=false",
                    "checkout", "--quiet", rmlui["commit"]], check=True)
    return source


def download(url: str, sha256: str) -> Path:
    """A pinned source archive, fetched once and checked every time."""
    DOWNLOADS.mkdir(parents=True, exist_ok=True)
    path = DOWNLOADS / url.rsplit("/", 1)[1]
    if not path.is_file():
        partial = path.with_suffix(path.suffix + f".{os.getpid()}")
        with urllib.request.urlopen(url) as response, partial.open("wb") as out:
            out.write(response.read())
        os.replace(partial, path)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != sha256:
        raise SystemExit(f"{path.name} is sha256 {digest}, the recipe pins {sha256}")
    return path


def freetype_prefix(destination: Path) -> Path:
    """Where the FreeType built in `destination` is installed."""
    return destination / recipe()["freetype"]["prefix"]


def build_freetype(destination: Path, target: str, jobs: int) -> Path:
    """FreeType built from its pinned release for `target`, installed in `destination`."""
    freetype = recipe()["freetype"]
    prefix = freetype_prefix(destination)
    if (prefix / "lib" / "pkgconfig" / "freetype2.pc").is_file():
        return prefix
    archive = download(freetype["url"], freetype["sha256"])
    with tarfile.open(archive) as tar:
        tar.extractall(destination, filter="data")
    environment = build_environment(target)
    source = destination / freetype["directory"]
    build = destination / freetype["build"]
    run(["cmake", "-S", str(source), "-B", str(build), "-G", "Ninja",
         f"-DCMAKE_INSTALL_PREFIX={prefix}", *freetype["cmake"], *cmake_flags(target)], destination, environment)
    run(["cmake", "--build", str(build), "--parallel", str(jobs)], destination, environment)
    run(["cmake", "--install", str(build)], destination, environment)
    return prefix


def freetype_environment(destination: Path) -> dict[str, str]:
    """Settings so that `pkg-config freetype2` returns the FreeType built in
    `destination`. We use LIBDIR, not PATH, so that nothing else on the machine
    is found, for freetype2 or any other library looked up in configure."""
    return {"PKG_CONFIG_LIBDIR": str(freetype_prefix(destination) / "lib" / "pkgconfig")}


def builds_sdl2(target: str) -> bool:
    """Whether we read controllers through SDL2 in `target`'s player, as set in the recipe."""
    return platform_of(require_target(target)) in recipe()["sdl2"]["platforms"]


def sdl2_prefix(destination: Path) -> Path:
    """Where the SDL2 built in `destination` is installed."""
    return destination / recipe()["sdl2"]["prefix"]


def release_source(destination: Path, name: str) -> Path:
    """The recipe's pinned release of the library `name`, unpacked in
    `destination` once."""
    release = recipe()[name]
    source = destination / release["directory"]
    if not source.is_dir():
        with tarfile.open(download(release["url"], release["sha256"])) as tar:
            tar.extractall(destination, filter="data")
    return source


def sdl2_source(destination: Path) -> Path:
    """The recipe's SDL2 release, unpacked in `destination` once."""
    return release_source(destination, "sdl2")


def build_sdl2(destination: Path, target: str, jobs: int) -> None:
    """SDL2 built from its pinned release for `target` and installed in
    `destination`, for a target with SDL2 in its recipe: for a player where we
    read controllers through it (builds_sdl2), or a launcher linked with it."""
    sdl2 = recipe()["sdl2"]
    prefix = sdl2_prefix(destination)
    # We build it again when the recipe's SDL2 changes, and then we compile
    # only what that change affects with cmake and ninja.
    stamp = prefix / "recipe.json"
    wanted = json.dumps({**sdl2, "target": cmake_flags(target)}, sort_keys=True)
    if stamp.is_file() and stamp.read_text(encoding="utf-8") == wanted:
        return
    source = sdl2_source(destination)
    environment = build_environment(target)
    build = destination / sdl2["build"]
    run(["cmake", "-S", str(source), "-B", str(build), "-G", "Ninja",
         f"-DCMAKE_INSTALL_PREFIX={prefix}", *sdl2["cmake"], *cmake_flags(target)], destination, environment)
    run(["cmake", "--build", str(build), "--parallel", str(jobs)], destination, environment)
    run(["cmake", "--install", str(build)], destination, environment)
    stamp.write_text(wanted, encoding="utf-8")


def sdl2_environment(destination: Path, target: str) -> dict[str, str]:
    """The settings for the SDL2 built in `destination` in configure: its
    header folder in INCLUDES, in configure's form (without the leading /),
    and in LDFLAGS the folder of the library and the libraries listed in its
    installed sdl2.pc. -lSDL2 is already added in configure."""
    if not builds_sdl2(target):
        return {}
    prefix = sdl2_prefix(destination)
    return {"INCLUDES": make_path(prefix / "include", target).lstrip("/"),
            "LDFLAGS": " ".join(sdl2_libraries(destination))}


def sdl2_include(destination: Path) -> str:
    """The compiler flag for the headers of the SDL2 built in `destination`, in
    the form that SDL programs include them (<SDL.h>)."""
    return f"-I{sdl2_prefix(destination) / 'include' / 'SDL2'}"


def sdl2_libraries(destination: Path) -> list[str]:
    """What we link next to -lSDL2 for the SDL2 built in `destination`, as
    listed in its sdl2.pc: the library folder and the system's libraries
    (-lSDL2 first). On Windows the .pc also lists SDL's main and the windowed
    subsystem for programs started through SDL, which ours are not."""
    described = (sdl2_prefix(destination) / "lib" / "pkgconfig" / "sdl2.pc").read_text(encoding="utf-8").splitlines()
    libs = next(line for line in described if line.startswith("Libs:"))[len("Libs:"):]
    own = {"-lSDL2", "-lSDL2main", "-mwindows"}
    return [flag for flag in libs.replace("${libdir}", (sdl2_prefix(destination) / "lib").as_posix()).split()
            if flag not in own]


def build_rmlui(destination: Path, target: str, jobs: int) -> Path:
    """RmlUi built statically at the recipe's commit; returns its build directory."""
    rmlui = recipe()["rmlui"]
    source = fetch_rmlui(destination)
    prefix = build_freetype(destination, target, jobs)
    build = destination / rmlui["build"]
    environment = {**build_environment(target), **freetype_environment(destination)}
    run(["cmake", "-S", str(source), "-B", str(build), "-G", "Ninja", *rmlui["cmake"],
         f"-DCMAKE_PREFIX_PATH={prefix}", *cmake_flags(target)], destination, environment)
    run(["cmake", "--build", str(build), "--parallel", str(jobs)], destination, environment)
    return build


DESKTOP = ROOT / "desktop/src-tauri"
LAUNCHER = DESKTOP / "launcher"
# The zips of a game's data, which the launcher, the player and the builder
# compile, and miniz, which we compile without our warnings.
GAMEDATA = DESKTOP / "gamedata"
ZIP_LIBRARY = GAMEDATA / "zip_library.c"
MINIZ = ROOT / "vendor/miniz"
# The libraries the game-data code is built on, which a program that compiles
# it compiles without our warnings: miniz, and libretro-common's JSON reader
# and writer for the manifest. The player has rjson of its own already.
LIBRETRO_COMMON = ROOT / "vendor/retroarch/libretro-common"
LIBRARIES = [ZIP_LIBRARY, LIBRETRO_COMMON / "formats/json/rjson.c", LIBRETRO_COMMON / "string/rstrtod.c"]


def platform_of(target: str) -> str:
    """The platform folder with the sources specific to a target."""
    if is_windows(target):
        return "windows"
    if is_macos(target):
        return "macos"
    raise SystemExit(f"no platform named for {target}")


def platform_sources(directory: Path, platform: str) -> list[Path]:
    """The C sources in `directory` on `platform`: the shared ones at its top,
    and those in the folders listed for the platform in the recipe's
    platformFolders."""
    folders = recipe()["platformFolders"].get(platform)
    if folders is None:
        raise SystemExit(f"the player recipe names no source folders for {platform}")
    return sorted(directory.glob("*.c")) + [source for folder in folders
                                            for source in sorted((directory / folder).glob("*.c"))]


def launcher_sources(platform: str) -> list[Path]:
    """The launcher's C sources on `platform`, with the zips of a game's data,
    without the LIBRARIES, which we compile without our warnings."""
    return [*platform_sources(LAUNCHER, platform), GAMEDATA / "game_data.c"]


def launcher_includes(platform: str) -> list[str]:
    """The folders the launcher's sources include from: its own, the
    platform's, the game data's, miniz's, the player's, for the menu's
    declarations, and libretro-common's, for rjson."""
    folders = [LAUNCHER, *(LAUNCHER / folder for folder in recipe()["platformFolders"][platform]), GAMEDATA, MINIZ,
               ROOT / "vendor/retroarch", LIBRETRO_COMMON / "include"]
    return [f"-I{folder}" for folder in folders]


def accounts_folder() -> Path:
    """Where the QUICK SIGN IN store's sources are."""
    return ROOT / recipe()["accounts"]["folder"]


def accounts_sources(platform: str) -> list[Path]:
    """The QUICK SIGN IN store's C sources on `platform`, laid out as the
    launcher's are."""
    return platform_sources(accounts_folder(), platform)


def launch_sources(platform: str) -> list[Path]:
    """The launcher's C sources for preparing a launch on `platform`, without
    its entry: the shared ones, the zips of a game's data, and the platform's
    file layer and path rules."""
    return [source for source in launcher_sources(platform)
            if source.parent in (LAUNCHER, GAMEDATA) or source.name in ("portable_fs.c", "paths.c")]


def file_layer(platform: str) -> list[Path]:
    """The C sources of the launcher's file layer (portable_fs.h) on
    `platform`: the sources common to every platform, and those for it."""
    return [source for source in launcher_sources(platform) if source.name == "portable_fs.c"]


def file_layer_headers(platform: str) -> list[Path]:
    """The headers of the launcher's file layer on `platform`: portable_fs.h,
    and the headers beside the platform's sources of the layer."""
    folders = {LAUNCHER, *(source.parent for source in file_layer(platform))}
    return sorted(header for folder in folders for header in folder.glob("portable_fs*.h"))


def copy_sources(destination: Path, target: str) -> Path:
    """The ROM-in-a-Box sources we compile into the player (the QUICK SIGN IN
    store and the zips of a game's data) and the file layer below them, in
    their folders under desktop/src-tauri, with the launcher's declarations
    they read, and miniz at the top of the folder, where the fork compiles it
    only through gamedata/zip_library.c."""
    handed = destination / "rominabox-sources"
    platform = platform_of(target)
    ours = [*accounts_sources(platform), *sorted(accounts_folder().glob("*.h")), GAMEDATA / "game_data.c",
            ZIP_LIBRARY, *sorted(GAMEDATA.glob("*.h")), LAUNCHER / "launch_contract.inc",
            *file_layer_headers(platform), *file_layer(platform)]
    copies = {handed / source.relative_to(DESKTOP): source for source in ours}
    copies.update({handed / source.name: source for source in (MINIZ / "miniz.c", MINIZ / "miniz.h")})
    for copy, source in copies.items():
        write_if_changed(copy, source.read_bytes())
    # In the fork's makefile we compile every source in the folders.
    for left in [path for path in handed.rglob("*") if path.is_file() and path not in copies]:
        left.unlink()
    return handed


def build_launcher(destination: Path, target: str, environment: dict[str, str]) -> Path | None:
    """The game's launcher, for a target where we build it next to the player,
    built into `destination`, with the libraries listed for it in the recipe
    built next to it."""
    launcher = recipe()["launcher"].get(require_target(target))
    if launcher is None:
        return None
    folder = destination / "launcher"
    output = folder / launcher["output"]
    includes = []
    theirs = []
    if launcher.get("zstd"):
        zstd = recipe()["zstd"]
        source = release_source(destination, "zstd")
        includes = [f"-I{source / path}" for path in zstd["includes"]]
        theirs = [(source / name, ["cc", "-std=gnu99", *compiler_flags(target), "-O2", "-w", *zstd["flags"], *includes])
                  for name in zstd["sources"]]
    theirs += [(library, ["cc", "-std=gnu99", *compiler_flags(target), "-O2", "-w",
                          *launcher_includes(platform_of(target))]) for library in LIBRARIES]
    own = [(source, ["cc", *compiler_flags(target), *launcher["flags"], *includes, *launcher_includes(platform_of(target))])
           for source in launcher_sources(platform_of(target))]
    sdl2 = []
    if launcher.get("sdl2"):
        build_sdl2(destination, target, os.cpu_count() or 1)
        own = [(source, [*compiler, sdl2_include(destination)]) for source, compiler in own]
        sdl2 = ["-lSDL2", *sdl2_libraries(destination)]
    steps = [compile_step(folder / "objects", index, source, compiler)
             for index, (source, compiler) in enumerate([*own, *theirs])]
    objects = [step.output for step in steps]
    steps.append(Step(output, objects, ["cc", *compiler_flags(target), *launcher["flags"], "-o", str(output),
                                        *map(str, objects), *sdl2, *launcher["libraries"]]))
    ninja(folder, steps, environment)
    return output


INJECTOR = ROOT / "scripts/native_runtime/inject_dylib.c"


def launch_library(kit: str) -> dict | None:
    """The launcher in a kit as a library loaded into its player (macOS), as
    declared in the recipe under the kit's target, or None for a kit with a
    launcher program built next to the player (Windows)."""
    return recipe()["launchLibrary"].get(require_build_target(kit))


def build_launch_library(destination: Path, kit: str) -> Path:
    """The kit's launch library, built into `destination` for every slice of
    the kit's player and for the platform's deployment target, with the
    install name that we load it by in the player."""
    declared = launch_library(kit)
    if declared is None:
        raise SystemExit(f"the player recipe declares no launch library for {kit}")
    output = destination / declared["output"]
    output.parent.mkdir(parents=True, exist_ok=True)
    processors = [flag for part in slices(kit) for flag in ("-arch", architecture_of(part).value)]
    run(["cc", *processors, f"-mmacosx-version-min={deployment_target(kit)}", *declared["flags"],
         f"-Wl,-install_name,{declared['loadedFrom']}/{declared['output']}", *launcher_includes(platform_of(kit)),
         "-o", str(output), *map(str, launcher_sources(platform_of(kit))), *map(str, LIBRARIES), *declared["libraries"]],
        destination, build_environment(kit))
    return output


def launch_library_sources(kit: str) -> str:
    """A digest of everything we build the kit's launch library from: the
    declaration in the recipe and every file included in its sources, as
    found by the compiler, with the player's headers among them. We record it
    in a kit and compare it with the tree's in the staging check."""
    declared = launch_library(kit)
    sources = [*launcher_sources(platform_of(kit)), *LIBRARIES]
    listed = subprocess.run(["cc", "-MM", *launcher_includes(platform_of(kit)), *map(str, sources)],
                            capture_output=True, text=True, check=True, cwd=ROOT).stdout
    # Make rules, `object: source header ...`. A trailing backslash continues
    # a rule on the next line, and a backslash before a space escapes it.
    words = listed.replace("\\\n", " ").replace("\\ ", "\0").split()
    files = sorted({(ROOT / word.replace("\0", " ")).resolve() for word in words if not word.endswith(":")})
    digest = hashlib.sha256(json.dumps([declared, deployment_target(kit)], sort_keys=True).encode())
    for file in files:
        digest.update(file.relative_to(ROOT).as_posix().encode() + b"\0" + file.read_bytes())
    return digest.hexdigest()


# Where we keep the launcher for kits and tests, built from this checkout's
# sources, in a folder for each kit. We build a Windows kit's launcher there
# with ninja, compiling again only what changed, and a macOS kit's launch
# library only when the digest of its sources changed. So in a run with many
# exports, or in a later run, we compile nothing again.
TREE_BUILDS = ROOT / "work/tree-builds"


@contextmanager
def tree_build(name: str) -> Iterator[Path]:
    """The folder in this checkout for the build `name`, locked by this process
    until the block ends, because we run test scopes in parallel, and we must
    not copy a program in one while writing it in another."""
    TREE_BUILDS.mkdir(parents=True, exist_ok=True)
    with (TREE_BUILDS / f"{name}.lock").open("a") as handle:
        file_lock.hold_exclusively(handle)
        folder = TREE_BUILDS / name
        folder.mkdir(exist_ok=True)
        yield folder


def kept_launcher(folder: Path, kit: str) -> Path:
    """The launcher for a kit for `kit`, built from this checkout's sources in
    `folder`, locked with tree_build: a Windows kit's program, or a macOS kit's
    launch library before we attach it to a player."""
    library = launch_library(kit)
    if library is None:
        built = build_launcher(folder, kit, build_environment(kit))
        if built is None:
            raise SystemExit(f"the player recipe builds no launcher for {kit}")
        return built
    built = folder / library["output"]
    record = folder / "sources.sha256"
    wanted = launch_library_sources(kit)
    if not built.is_file() or not record.is_file() or record.read_text(encoding="utf-8") != wanted:
        build_launch_library(folder, kit)
        record.write_text(wanted, encoding="utf-8")
    return built


def tree_launcher(kit: str, destination: Path) -> Path:
    """The launcher for a kit for `kit`, built from this checkout's sources and
    copied to the file `destination`."""
    with tree_build(kit) as folder:
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(kept_launcher(folder, kit), destination)
    return destination


def install_tree_launcher(kit_folder: Path, kit: str) -> None:
    """The kit for `kit` at `kit_folder`, given the launcher built from this
    checkout's sources: a Windows kit's program next to its player, or a macOS
    kit's launch library, attached to the kit's player there."""
    files = recipe()["kit"][kit]["files"]
    tree_launcher(kit, kit_folder / files["launcher"]["at"])
    if launch_library(kit):
        with tempfile.TemporaryDirectory(prefix="rominabox-launcher-") as workspace:
            attach_launch_library(kit_folder / files["player"]["at"], kit, Path(workspace))


def attach_launch_library(player: Path, kit: str, workspace: Path) -> None:
    """Attach the kit's launch library to every slice of the macOS `player`, to
    be loaded before main and to prepare the arguments of main, then sign the
    player ad hoc, as required for arm64 code on a Mac. We build the injector
    for this Mac, where we run it, whatever the player is for."""
    declared = launch_library(kit)
    if declared is None:
        raise SystemExit(f"the player recipe declares no launch library for {kit}")
    injector = workspace / "inject-dylib"
    run(["cc", "-Oz", "-o", str(injector), str(INJECTOR)], workspace, dict(os.environ))
    loaded = f"{declared['loadedFrom']}/{declared['output']}"
    held = sorted(architecture.value for architecture in architectures_in(player))
    if len(held) == 1:
        run([str(injector), str(player), loaded], workspace, dict(os.environ))
    else:
        parts = []
        for architecture in held:
            part = workspace / f"player-{architecture}"
            run(["lipo", str(player), "-thin", architecture, "-output", str(part)], workspace, dict(os.environ))
            run([str(injector), str(part), loaded], workspace, dict(os.environ))
            parts.append(str(part))
        run(["lipo", "-create", "-output", str(player), *parts], workspace, dict(os.environ))
    run(["codesign", "--force", "--sign", "-", str(player)], workspace, dict(os.environ))


def rmlui_linking(makefile: Path, source: Path) -> tuple[str, list[Path], list[str]]:
    """The RmlUi settings in the player's makefile: the archive we link (a
    name in RmlUi's build directory), its header directories under `source`,
    and the defines for its compilation, such as the static-library one.
    Without RMLUI_STATIC_LIB, every function in the RmlUi headers is declared
    as imported from a DLL on Windows, and the archive is never linked."""
    import re

    if not makefile.is_file():
        raise SystemExit(f"missing {makefile}")
    text = makefile.read_text(encoding="utf-8", errors="replace")
    archive = re.search(r"\$\(RMLUI_BUILD_DIR\)/(\S+)", text)
    if not archive:
        raise SystemExit(f"{makefile} no longer links an RmlUi archive")
    headers: list[Path] = []
    for name in re.findall(r"\$\(RMLUI_SOURCE_DIR\)/(\S+)", text):
        if source / name not in headers:
            headers.append(source / name)
    if not headers:
        raise SystemExit(f"{makefile} no longer names RmlUi header directories")
    defines = list(dict.fromkeys(re.findall(r"DEFINES \+= (-DRMLUI_\w+)", text)))
    return archive.group(1), headers, defines


PREVIEW = ROOT / "desktop/src-tauri/preview"


def preview_resource(target: str) -> Path:
    """Where the menu preview renderer for `target` is in the builder's
    resources, under the name of the program in the recipe."""
    declared = recipe()["preview"].get(require_target(target))
    if declared is None:
        raise SystemExit(f"no menu preview is built for {target}")
    return ROOT / "desktop/src-tauri/resources/preview" / declared["output"]


def build_preview(destination: Path, target: str, environment: dict[str, str], rmlui_build: Path) -> Path | None:
    """The builder's menu preview renderer, from the fork checked out in
    `destination` and the RmlUi and FreeType built there, for a target listed
    in the recipe."""
    declared = recipe()["preview"]
    platform = declared.get(require_target(target))
    if platform is None:
        return None
    fork = destination / "retroarch"
    archive, headers, defines = rmlui_linking(fork / "Makefile.common", destination / recipe()["rmlui"]["source"])
    context = platform["context"]
    context_sources = [ROOT / name for name in context["sources"]]
    own = [*(ROOT / name for name in declared["sources"]), ROOT / platform["entry"], *context_sources]
    forked = [fork / name for name in (*recipe()["fileLayer"]["sources"], *declared["forkSources"],
                                       *context["forkSources"])]
    freetype = subprocess.run([resolve("pkg-config", environment), "--cflags", "--libs", "freetype2"],
                              capture_output=True, text=True, check=True, env=environment).stdout.split()
    includes = [f"-I{path}" for path in (PREVIEW, fork / "menu/drivers", fork / "libretro-common/include", fork,
                                         *headers)]
    folder = destination / "preview"
    steps = []
    for index, source in enumerate([*own, *forked]):
        language = {".c": ["cc", "-std=gnu99"], ".cpp": ["c++", "-std=c++17"],
                    ".mm": ["c++", "-std=c++17", "-x", "objective-c++"]}[source.suffix]
        extra = context["flags"] if source in context_sources else []
        steps.append(compile_step(folder / "objects", index, source,
                                  [*language, *compiler_flags(target), "-O2", *defines, *includes, *freetype, *extra]))
    objects = [step.output for step in steps]
    output = folder / platform["output"]
    unstripped = linked(output)
    steps.append(Step(unstripped, [*objects, rmlui_build / archive],
                      ["c++", *compiler_flags(target), *platform["flags"], "-o", str(unstripped), *map(str, objects),
                       str(rmlui_build / archive), *freetype, *context["libraries"]]))
    steps.append(Step(output, [unstripped], ["strip", "-o", str(output), str(unstripped)]))
    ninja(folder, steps, environment)
    return output


def has_symbol(binary: Path, target: str, function: str, environment: dict[str, str]) -> bool:
    """Whether `binary` exports `function`; in a universal file, its slice for `target`."""
    listed = subprocess.run([resolve("nm", environment), "-g", *slice_selection(target), str(binary)],
                            capture_output=True, text=True, check=True, env=environment).stdout
    wanted = symbol_prefix(target) + function
    return any(line.split()[-1:] == [wanted] for line in listed.splitlines())


def joypad_profile_drivers(platform_name: str) -> list[str]:
    """The controller profile folders for the player of a platform."""
    declared = recipe()["drivers"].get(platform_name)
    if declared is None:
        raise SystemExit(f"the player recipe declares no drivers for {platform_name}")
    return declared["joypadProfiles"]


def system_libraries(target: str) -> set[str]:
    """The libraries present on every machine of `target`'s platform: on
    Windows their names in lower case, on macOS their folders. We let a player
    or a launcher link these and nothing else, because we would have to ship
    anything else next to it."""
    return set(recipe()["systemLibraries"][platform_of(target)])


def foreign_imports(binary: Path, target: str, environment: dict[str, str]) -> list[str]:
    """Libraries linked into `binary` (in a universal file, its slice for
    `target`) that are not part of the target's system, so that we would have
    to ship them next to it."""
    allowed = system_libraries(target)
    if is_windows(target):
        dumped = subprocess.run([resolve("objdump", environment), "-p", str(binary)], capture_output=True,
                                text=True, check=True, env=environment).stdout
        names = [line.split("DLL Name:")[1].strip() for line in dumped.splitlines() if "DLL Name:" in line]
        return [name for name in names
                if name.lower() not in allowed and not name.lower().startswith("api-ms-win-")]
    if is_macos(target):
        listed = subprocess.run(["otool", "-L", *slice_selection(target), str(binary)], capture_output=True,
                                text=True, check=True, env=environment).stdout
        # The first line contains the file name (and, for a slice, its processor).
        paths = [line.strip().split(" (")[0] for line in listed.splitlines()[1:]]
        return [path for path in paths if not path.startswith(tuple(allowed))]
    raise SystemExit(f"no way to read the libraries a {target} binary links")


def slice_selection(target: str) -> list[str]:
    """The options of Apple's nm and otool for the slice for `target` in a
    universal file, or nothing on a platform with one slice per file."""
    if is_macos(target):
        return ["-arch", architecture_of(target).value]
    if is_windows(target):
        return []
    raise SystemExit(f"no slice selection for {target}")


def architectures_in(binary: Path) -> set[Architecture]:
    """The processors with a slice in a Mach-O file, read from the file."""
    listed = subprocess.run(["lipo", "-archs", str(binary)], capture_output=True, text=True, check=True).stdout
    return {Architecture(name) for name in listed.split()}
