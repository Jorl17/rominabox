"""Build the builder: the ROM-in-a-Box authoring app, with its runtime kit,
its menu preview renderer and its command line inside. We only build here,
and never start the builder or an emulator.

    uv run python scripts/build_builder.py
    uv run python scripts/build_builder.py --mac x86_64      # or arm64, or universal

On macOS, --mac chooses the processors of the builder: arm64, x86_64, or
universal for both. Without it, we build for this Mac's own processor. The
builder program, its command line and its menu preview renderer are then
built for those processors, and the runtime kit inside stays universal,
because games made with any builder can run on Intel Macs.

Build the kit and the preview renderer first: scripts/build_player.py for a
player that ships, then scripts/build_kit.py with that build. What differs
by platform is in PLATFORMS: how Tauri bundles the app and what we do to it
afterwards. Everything else is the same everywhere.
"""

from __future__ import annotations

import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import kit_assets  # noqa: E402
import native_build  # noqa: E402
from built import NAME as CLI_NAME, cli_build, target_dir  # noqa: E402
from native_build import Architecture  # noqa: E402
from core_source import host_target  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "desktop/src-tauri"
RESOURCES = TAURI / "resources"
KIT = RESOURCES / "runtime"
# productName in desktop/src-tauri/tauri.conf.json, and the crate's program.
PRODUCT = "ROM-in-a-Box"
PROGRAM = "rominabox-desktop"
# The Rust target of each Mac processor, and the processors of each choice of
# --mac. Tauri builds a universal app itself from both targets.
RUST_TARGETS = {Architecture.ARM64: "aarch64-apple-darwin", Architecture.X86_64: "x86_64-apple-darwin"}
MAC_CHOICES = {"arm64": [Architecture.ARM64], "x86_64": [Architecture.X86_64],
               "universal": [Architecture.ARM64, Architecture.X86_64]}


def mach_o(path: Path) -> bool:
    """Whether `path` is a Mach-O file, which we sign separately. The output
    of file(1) for other files can contain any byte (the copyright sign of a
    font in Latin-1), and we use only the start of the output."""
    kind = subprocess.run(["/usr/bin/file", "-b", "--", str(path)], capture_output=True,
                          encoding="utf-8", errors="replace",
                          env={**os.environ, "LC_ALL": "C"}, check=True).stdout
    return kind.startswith("Mach-O")


def tauri_target(processors: list[Architecture] | None) -> str | None:
    """The target we pass to Tauri for `processors`, or None for this
    machine's own."""
    if not processors:
        return None
    return RUST_TARGETS[processors[0]] if len(processors) == 1 else "universal-apple-darwin"


def bundle_folder(built: Path, processors: list[Architecture] | None) -> Path:
    """Where Cargo and Tauri write a release build for `processors`."""
    target = tauri_target(processors)
    return built / target / "release" if target else built / "release"


def thin_preview(app: Path, processors: list[Architecture]) -> None:
    """Keep only `processors` in the menu preview renderer inside `app`. The
    renderer in the kit is universal. The player and launcher in the kit
    stay universal for the games."""
    renderer = app / "Contents/Resources/preview/rml-preview"
    if len(processors) == 1:
        subprocess.run(["lipo", "-thin", processors[0].value, "-output", str(renderer), str(renderer)], check=True)


def macos_finish(built: Path, processors: list[Architecture] | None = None) -> Path:
    """The .app, signed after Tauri copied the resources in: each actual Mach-O
    leaf first, including libraries without the execute bit, then the bundle.
    --deep signing is no substitute. When ROMINABOX_SIGN_IDENTITY is set to a
    Developer ID certificate in the keychain, we sign with that certificate,
    the hardened runtime and a timestamp, as required for notarization. When
    it is unset, we sign ad hoc."""
    app = bundle_folder(built, processors) / "bundle/macos" / f"{PRODUCT}.app"
    if not app.is_dir():
        raise SystemExit(f"missing builder bundle: {app}")
    if processors:
        thin_preview(app, processors)
    identity = os.environ.get("ROMINABOX_SIGN_IDENTITY", "")
    sign = ["/usr/bin/codesign", "--force", "--sign", identity or "-"]
    if identity:
        sign += ["--options", "runtime", "--timestamp"]
    for path in sorted(app.rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        if mach_o(path):
            subprocess.run([*sign, "--", str(path)], check=True)
    subprocess.run([*sign, "--", str(app)], check=True)
    subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", "--", str(app)], check=True)
    return app


def windows_finish(built: Path, processors: list[Architecture] | None = None) -> Path:
    """The installer Tauri built: one setup program for installing the
    builder for the person, with its resources, and WebView2 only where
    Windows lacks it (tauri.conf.json). The program and the command line
    inside it may import only what comes with Windows."""
    program = built / "release" / f"{PROGRAM}.exe"
    if not program.is_file():
        raise SystemExit(f"missing builder program: {program}")
    installers = sorted((built / "release/bundle/nsis").glob("*-setup.exe"), key=lambda path: path.stat().st_mtime)
    if not installers:
        raise SystemExit(f"missing builder installer in {built / 'release/bundle/nsis'}")
    environment = native_build.build_environment(host_target())
    for binary in (program, RESOURCES / "bin" / CLI_NAME):
        foreign = native_build.foreign_imports(binary, host_target(), environment)
        if foreign:
            raise SystemExit(f"{binary.name} needs DLLs Windows does not have: {', '.join(foreign)}")
    return installers[-1]


# How we bundle and finish the builder on each platform: a signed .app on
# macOS, an NSIS installer on Windows. `staged` lists libraries that may be
# read-only in the Tauri staging folder from an earlier build.
PLATFORMS = {
    "macos": {"bundle": ["--bundles", "app"], "staged": "*.dylib", "finish": macos_finish},
    "windows": {"bundle": ["--bundles", "nsis"], "staged": None, "finish": windows_finish},
}


def cargo_output() -> Path:
    """Where Cargo writes the builder and its command line: the target that
    we get from built.target_dir in every build script, which in a worktree is
    shared and outside the checkout. We resolve it from the working directory
    of Cargo, so a relative CARGO_TARGET_DIR is the same folder during the
    build and the steps after it."""
    built = target_dir()
    return (built if built.is_absolute() else TAURI / built).resolve()


def writable(folder: Path, pattern: str = "*") -> None:
    """In a Tauri build, resources keep their permissions, and a frozen
    library can be read-only, so we make the copies writable for later builds."""
    for path in folder.rglob(pattern):
        if path.is_file() and not path.is_symlink() and not os.access(path, os.W_OK):
            path.chmod(path.stat().st_mode | stat.S_IWRITE)


def main() -> int:
    arguments = sys.argv[1:]
    processors = None
    if arguments[:1] == ["--mac"] and len(arguments) == 2 and arguments[1] in MAC_CHOICES:
        processors = MAC_CHOICES[arguments[1]]
    elif arguments:
        raise SystemExit(__doc__)
    target = host_target()
    platform = PLATFORMS.get(target.split("-", 1)[0])
    if platform is None:
        raise SystemExit(f"the builder is not built for {target}")
    if processors and not target.startswith("macos-"):
        raise SystemExit("--mac chooses the processors of a builder built on a Mac")
    if not (KIT / "manifest.json").is_file():
        raise SystemExit(f"prepare the runtime kit first: no {KIT / 'manifest.json'}")
    renderer = native_build.preview_resource(target)
    if not renderer.is_file():
        raise SystemExit(f"prepare the menu preview renderer first: no {renderer}")

    built = cargo_output()
    # We stage the assets of the kit the same way for every platform.
    kit_assets.stage(KIT)
    writable(RESOURCES)
    if platform["staged"]:
        for profile in ("debug", "release"):
            if (built / profile).is_dir():
                writable(built / profile, platform["staged"])

    (RESOURCES / "bin").mkdir(parents=True, exist_ok=True)
    if processors:
        programs = []
        for processor in processors:
            subprocess.run([*cli_build(), "--target", RUST_TARGETS[processor]], cwd=TAURI, check=True)
            programs.append(str(built / RUST_TARGETS[processor] / "release" / CLI_NAME))
        subprocess.run(["lipo", "-create", "-output", str(RESOURCES / "bin" / CLI_NAME), *programs], check=True)
    else:
        subprocess.run(cli_build(), cwd=TAURI, check=True)
        shutil.copy2(built / "release" / CLI_NAME, RESOURCES / "bin" / CLI_NAME)

    npm = shutil.which("npm")
    if not npm:
        raise SystemExit("npm is not installed")
    target_arguments = ["--target", tauri_target(processors)] if processors else []
    subprocess.run([npm, "run", "tauri", "build", "--", *platform["bundle"], *target_arguments],
                   cwd=ROOT / "desktop", check=True)
    print(f"Built {platform['finish'](built, processors)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
