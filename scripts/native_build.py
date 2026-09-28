"""The player's native build: one recipe, and the steps common to all targets.

`scripts/native_runtime/player-recipe.json` lists what we build, and in this
module we build it, for the player (`scripts/build_player.py`) and for the
RmlUi linked into the tests (`scripts/prepare_rmlui.py`), so the two builds
cannot differ. Targets differ only where the recipe has a key for a platform
or a target or a function here is specific to one: the shell for configure,
the processor and system version of a slice, the name of the finished
binary, the spelling of its symbols and the libraries we may link into it.
"""

from __future__ import annotations

import enum
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import toolchain  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
RECIPE_PATH = ROOT / "scripts/native_runtime/player-recipe.json"
FORK = ROOT / "vendor/retroarch"
DOWNLOADS = ROOT / "work/downloads"


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


def makefile_local(target: str) -> str:
    makefile = recipe()["makefile"]
    return "\n".join([*makefile["common"], *makefile[platform_of(require_target(target))]]) + "\n"


def make_variables(target: str) -> list[str]:
    """The command-line settings for the fork's makefile to build a slice for
    its processor and system version. With its ARCH switch we add -arch to
    every compile and the link, and with MINVERFLAGS we replace the default
    version for that processor."""
    if is_macos(target):
        return [f"ARCH={architecture_of(target).value}",
                f"MINVERFLAGS=-mmacosx-version-min={deployment_target(target)}"]
    if is_windows(target):
        return []
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


def shell(target: str) -> list[str]:
    """The POSIX shell in which we run the RetroArch configure script."""
    if is_windows(target):
        return [str(toolchain.msys2_root() / "usr" / "bin" / "bash.exe")]
    if is_macos(target):
        return ["/bin/sh"]
    raise SystemExit(f"no shell for {target}")


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


def fork_commit() -> str:
    if subprocess.run(["git", "-C", str(FORK), "status", "--porcelain"],
                      capture_output=True, text=True, check=True).stdout.strip():
        raise SystemExit("commit native source changes before building")
    return subprocess.run(["git", "-C", str(FORK), "rev-parse", "HEAD"],
                          capture_output=True, text=True, check=True).stdout.strip()


def archive_fork(commit: str, destination: Path, target: str) -> None:
    """The committed fork, with LF line endings whatever the checkout has."""
    with tempfile.TemporaryDirectory() as temporary:
        archive = Path(temporary) / "retroarch.tar"
        subprocess.run(["git", "-c", "core.autocrlf=false", "-C", str(FORK), "archive",
                        "--format=tar", "--prefix=retroarch/", "-o", str(archive), commit], check=True)
        with tarfile.open(archive) as tar:
            members = tar.getmembers()
            if is_windows(target):
                # On Windows, creating symbolic links requires extra rights.
                # The only ones in the fork are inside Apple framework bundles.
                skipped = [member.name for member in members if member.issym() or member.islnk()]
                if any("pkg/apple/" not in name for name in skipped):
                    raise SystemExit(f"the fork has symbolic links outside Apple bundles: {skipped}")
                members = [member for member in members if not (member.issym() or member.islnk())]
            tar.extractall(destination, members=members, filter="data")


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


def platform_of(target: str) -> str:
    """The platform folder with the sources specific to a target."""
    if is_windows(target):
        return "windows"
    if is_macos(target):
        return "macos"
    raise SystemExit(f"no platform named for {target}")


def launcher_sources(platform: str) -> list[Path]:
    """The launcher's C sources on `platform`: the shared ones at the top of
    LAUNCHER, and those in the folders listed in the recipe for the platform."""
    folders = recipe()["launcher"]["folders"].get(platform)
    if folders is None:
        raise SystemExit(f"the player recipe names no launcher folders for {platform}")
    return sorted(LAUNCHER.glob("*.c")) + [source for folder in folders
                                           for source in sorted((LAUNCHER / folder).glob("*.c"))]


def file_layer(platform: str) -> list[Path]:
    """The C sources of the launcher's file layer (portable_fs.h) on
    `platform`: the sources common to every platform, and those for it."""
    return [source for source in launcher_sources(platform) if source.name == "portable_fs.c"]


def copy_accounts(destination: Path, target: str) -> Path:
    """The ROM-in-a-Box sources we compile into the player (the QUICK SIGN IN
    store) and the file layer below it, in their folders under
    desktop/src-tauri."""
    accounts = destination / "rominabox-accounts"
    sources = [ROOT / relative for relative in recipe()["accounts"]["sources"]]
    for source in [*sources, LAUNCHER / "portable_fs.h", *file_layer(platform_of(target))]:
        copy = accounts / source.relative_to(DESKTOP)
        copy.parent.mkdir(parents=True, exist_ok=True)
        copy.write_bytes(source.read_bytes())
    return accounts


def build_launcher(destination: Path, target: str, environment: dict[str, str]) -> Path | None:
    """The game's launcher, for a target where we build it next to the player."""
    launcher = recipe()["launcher"].get(require_target(target))
    if launcher is None:
        return None
    sources = launcher_sources(platform_of(target))
    output = destination / "launcher" / launcher["output"]
    output.parent.mkdir(parents=True, exist_ok=True)
    run(["cc", *compiler_flags(target), *launcher["flags"], "-o", str(output), *map(str, sources),
         *launcher["libraries"]], destination, environment)
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
         f"-Wl,-install_name,{declared['loadedFrom']}/{declared['output']}",
         "-o", str(output), *map(str, launcher_sources(platform_of(kit)))], destination, build_environment(kit))
    return output


def launch_library_sources(kit: str) -> str:
    """A digest of everything we build the kit's launch library from: the
    declaration in the recipe and every file included in its sources, as
    found by the compiler, with the player's headers among them. We record it
    in a kit and compare it with the tree's in the staging check."""
    declared = launch_library(kit)
    sources = launcher_sources(platform_of(kit))
    listed = subprocess.run(["cc", "-MM", *map(str, sources)], capture_output=True, text=True, check=True,
                            cwd=ROOT).stdout
    # Make rules, `object: source header ...`. A trailing backslash continues
    # a rule on the next line, and a backslash before a space escapes it.
    words = listed.replace("\\\n", " ").replace("\\ ", "\0").split()
    files = sorted({(ROOT / word.replace("\0", " ")).resolve() for word in words if not word.endswith(":")})
    digest = hashlib.sha256(json.dumps([declared, deployment_target(kit)], sort_keys=True).encode())
    for file in files:
        digest.update(file.relative_to(ROOT).as_posix().encode() + b"\0" + file.read_bytes())
    return digest.hexdigest()


def install_launch_library(kit_folder: Path, kit: str, workspace: Path) -> None:
    """The kit's launch library, built from the tree into the kit at
    `kit_folder`, and attached to the kit's player there."""
    files = recipe()["kit"][kit]["files"]
    library = build_launch_library(workspace, kit)
    (kit_folder / files["launcher"]["at"]).parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(library, kit_folder / files["launcher"]["at"])
    attach_launch_library(kit_folder / files["player"]["at"], kit, workspace)


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
    """The builder's menu preview renderer, from the fork archived in
    `destination` and the RmlUi and FreeType built there, for a target listed
    in the recipe."""
    declared = recipe()["preview"]
    platform = declared.get(require_target(target))
    if platform is None:
        return None
    fork = destination / "retroarch"
    archive, headers, defines = rmlui_linking(fork / "Makefile.common", destination / recipe()["rmlui"]["source"])
    context = platform["context"]
    own = [ROOT / name for name in (*declared["sources"], *context["sources"])]
    forked = [fork / name for name in (*recipe()["fileLayer"]["sources"], *declared["forkSources"],
                                       *context["forkSources"])]
    freetype = subprocess.run([resolve("pkg-config", environment), "--cflags", "--libs", "freetype2"],
                              capture_output=True, text=True, check=True, env=environment).stdout.split()
    includes = [f"-I{path}" for path in (PREVIEW, fork / "menu/drivers", fork / "libretro-common/include", fork,
                                         *headers)]
    objects_dir = destination / "preview" / "objects"
    objects_dir.mkdir(parents=True, exist_ok=True)
    objects = []
    for index, source in enumerate([*own, *forked]):
        language = {".c": ["cc", "-std=gnu99"], ".cpp": ["c++", "-std=c++17"],
                    ".mm": ["c++", "-std=c++17", "-x", "objective-c++"]}[source.suffix]
        extra = context["flags"] if source in own[len(declared["sources"]):] else []
        obj = objects_dir / f"{index:02d}-{source.stem}.o"
        run([*language, *compiler_flags(target), "-O2", *defines, *includes, *freetype, *extra,
             "-c", str(source), "-o", str(obj)], destination, environment)
        objects.append(obj)
    output = destination / "preview" / platform["output"]
    run(["c++", *compiler_flags(target), *platform["flags"], "-o", str(output), *map(str, objects),
         str(rmlui_build / archive), *freetype, *context["libraries"]], destination, environment)
    run(["strip", str(output)], destination, environment)
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
