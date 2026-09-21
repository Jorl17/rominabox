"""Build a macOS command-line RetroArch test runtime without Cocoa windows or desktop input."""

import argparse
import json
import os
import shutil
import subprocess
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OPTIONS = """--disable-cocoa --disable-retroarch_playlist_manager --disable-mfi --disable-avf
--disable-opengl --disable-opengl1 --disable-opengl_core --disable-metal --disable-vulkan
--disable-sdl --disable-sdl2 --disable-qt --enable-menu --enable-rgui --disable-xmb
--disable-ozone --disable-materialui --disable-gfx_widgets --disable-coreaudio --disable-microphone
--disable-hid --disable-libusb --disable-networking --disable-accessibility --disable-discord
--disable-ffmpeg --disable-mpv --disable-x11 --disable-wayland --disable-cg --disable-cheevos
--enable-threads --disable-freetype --disable-alsa --disable-pulse --disable-jack --disable-oss
--disable-audioio --disable-rsound --disable-roar --disable-pipewire --disable-al --disable-test_drivers""".split()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-kit", type=Path, default=ROOT / "work/runtime-kit")
    args = parser.parse_args()
    kit = args.runtime_kit.resolve()
    source = ROOT / "work/headless-source/RetroArch-e33bb934"
    archive = kit / "sources/RetroArch-e33bb934.tar.gz"
    if not archive.exists():
        parser.error("Prepare the runtime kit first.")
    # Extract afresh, so the small build-only patch is reproducible and no objects are stale.
    if source.exists():
        shutil.rmtree(source)
    with tarfile.open(archive) as bundle:
        bundle.extractall(source.parent, filter="data")
    patch = ROOT / "scripts/patches/retroarch-headless.patch"
    commands = [
        ["patch", "-p1", "-i", str(patch)],
        ["./configure", *OPTIONS],
        [
            "make",
            "-j6",
            "CPPFLAGS=-DOSX",
            "LDFLAGS=-framework Foundation -framework IOKit -framework Carbon -framework AVFoundation",
        ],
    ]
    for index, command in enumerate(commands):
        print(f"Headless build step {index + 1}/{len(commands)}", flush=True)
        with (ROOT / f"work/headless-step-{index + 1}.log").open("w") as log:
            subprocess.run(
                command,
                cwd=source,
                env=dict(os.environ, CFLAGS="-O2 -DOSX"),
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
            )
    symbols = subprocess.check_output(["nm", str(source / "retroarch")], text=True)
    if any(
        symbol in symbols for symbol in ("_OBJC_CLASS_$_NSApplication", "_OBJC_CLASS_$_NSWindow", "RAWindow", "_NSApp")
    ):
        raise RuntimeError("Headless binary contains desktop window symbols")
    target = kit / "headless"
    target.mkdir(exist_ok=True)
    shutil.copy2(source / "retroarch", target / "retroarch")
    subprocess.run(["codesign", "--force", "--sign", "-", str(target / "retroarch")], check=True)
    shutil.copy2(patch, target / patch.name)
    (target / "build.json").write_text(
        json.dumps(
            {
                "revision": "e33bb934",
                "commands": commands,
                "purpose": "Local automated tests only; no Cocoa frontend; not used for exported gameplay",
            },
            indent=2,
        )
    )
    print(target / "retroarch")


if __name__ == "__main__":
    main()
