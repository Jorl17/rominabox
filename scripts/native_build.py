"""The player's native build: one recipe, and the steps common to all targets.

`scripts/native_runtime/player-recipe.json` lists what we build, and in this
module we build it, for the player (`scripts/build_player.py`) and for the
RmlUi linked into the tests (`scripts/prepare_rmlui.py`), so the two builds
cannot differ. Targets differ only where the recipe has a key for a target
or a function here is specific to one: the shell for configure, the source
of FreeType, the name of the finished binary and the spelling of its symbols.
"""

from __future__ import annotations

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
    if target not in recipe()["targets"]:
        raise SystemExit(f"the player recipe has no target {target}; it has {', '.join(recipe()['targets'])}")
    return target


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
    return [*configure["common"], *configure.get(require_target(target), [])]


def makefile_local(target: str) -> str:
    makefile = recipe()["makefile"]
    return "\n".join([*makefile["common"], *makefile.get(require_target(target), [])]) + "\n"


def build_environment(target: str) -> dict[str, str]:
    """The environment in which we run configure, make, cmake and the compilers."""
    environment = dict(os.environ)
    if is_windows(target):
        root = toolchain.msys2_root()
        environment["MSYSTEM"] = "UCRT64"
        environment["PATH"] = os.pathsep.join(
            [str(root / "ucrt64" / "bin"), str(root / "usr" / "bin"), environment.get("PATH", "")]
        )
    elif not is_macos(target):
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


def freetype_prefix(destination: Path, target: str) -> Path | None:
    """Where this target's FreeType is installed, or None when the system's is used."""
    freetype = recipe()["freetype"][require_target(target)]
    if freetype["from"] == "pkg-config":
        return None
    return destination / freetype["prefix"]


def build_freetype(destination: Path, target: str, jobs: int) -> Path | None:
    """FreeType built from its pinned release, for a target without the system's."""
    freetype = recipe()["freetype"][require_target(target)]
    if freetype["from"] == "pkg-config":
        return None
    if freetype["from"] != "source":
        raise SystemExit(f"unknown FreeType source {freetype['from']!r} for {target}")
    prefix = destination / freetype["prefix"]
    if (prefix / "lib" / "pkgconfig" / "freetype2.pc").is_file():
        return prefix
    archive = download(freetype["url"], freetype["sha256"])
    with tarfile.open(archive) as tar:
        tar.extractall(destination, filter="data")
    environment = build_environment(target)
    source = destination / freetype["directory"]
    build = destination / freetype["build"]
    run(["cmake", "-S", str(source), "-B", str(build), "-G", "Ninja",
         f"-DCMAKE_INSTALL_PREFIX={prefix}", *freetype["cmake"]], destination, environment)
    run(["cmake", "--build", str(build), "--parallel", str(jobs)], destination, environment)
    run(["cmake", "--install", str(build)], destination, environment)
    return prefix


def freetype_environment(destination: Path, target: str) -> dict[str, str]:
    """Settings so that `pkg-config freetype2` returns this target's FreeType."""
    prefix = freetype_prefix(destination, target)
    if prefix is None:
        return {}
    # LIBDIR, not PATH, so that no other freetype2 on the machine is found.
    return {"PKG_CONFIG_LIBDIR": str(prefix / "lib" / "pkgconfig")}


def build_rmlui(destination: Path, target: str, jobs: int) -> Path:
    """RmlUi built statically at the recipe's commit; returns its build directory."""
    rmlui = recipe()["rmlui"]
    source = fetch_rmlui(destination)
    prefix = build_freetype(destination, target, jobs)
    build = destination / rmlui["build"]
    environment = {**build_environment(target), **freetype_environment(destination, target)}
    extra = [f"-DCMAKE_PREFIX_PATH={prefix}"] if prefix else []
    run(["cmake", "-S", str(source), "-B", str(build), "-G", "Ninja", *rmlui["cmake"], *extra],
        destination, environment)
    run(["cmake", "--build", str(build), "--parallel", str(jobs)], destination, environment)
    return build


def copy_accounts(destination: Path) -> Path:
    """The ROM-in-a-Box sources we compile into the player (the QUICK SIGN IN store)."""
    accounts = destination / "rominabox-accounts"
    for folder, files in recipe()["accounts"].items():
        (accounts / folder).mkdir(parents=True, exist_ok=True)
        for relative in files:
            source = ROOT / relative
            (accounts / folder / source.name).write_bytes(source.read_bytes())
    return accounts


def has_symbol(binary: Path, target: str, function: str, environment: dict[str, str]) -> bool:
    listed = subprocess.run([resolve("nm", environment), "-g", str(binary)], capture_output=True, text=True,
                            check=True, env=environment).stdout
    wanted = symbol_prefix(target) + function
    return any(line.split()[-1:] == [wanted] for line in listed.splitlines())


# The DLLs we allow a Windows player to import, which are those of Windows.
# Anything else we would have to ship next to the player, and we avoid that.
WINDOWS_SYSTEM_DLLS = {
    "advapi32.dll", "comdlg32.dll", "crypt32.dll", "dinput8.dll", "dsound.dll", "gdi32.dll",
    "hid.dll", "imm32.dll", "iphlpapi.dll", "kernel32.dll", "msimg32.dll", "ole32.dll",
    "opengl32.dll", "setupapi.dll", "shell32.dll", "user32.dll", "winmm.dll", "ws2_32.dll",
    "xinput1_4.dll", "bcrypt.dll", "shlwapi.dll", "version.dll", "dwmapi.dll", "uxtheme.dll",
    "oleaut32.dll", "cfgmgr32.dll", "ntdll.dll",
}


def foreign_imports(binary: Path, environment: dict[str, str]) -> list[str]:
    """DLLs a Windows binary imports that are not part of Windows."""
    dumped = subprocess.run([resolve("objdump", environment), "-p", str(binary)], capture_output=True, text=True,
                            check=True, env=environment).stdout
    names = [line.split("DLL Name:")[1].strip() for line in dumped.splitlines() if "DLL Name:" in line]
    return [name for name in names
            if name.lower() not in WINDOWS_SYSTEM_DLLS and not name.lower().startswith("api-ms-win-")]
