import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import tarfile
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

CORES = {
    "gambatte": ("libretro/gambatte-libretro", "d9d6cd06382d1ced30de34d56d3609452323dab1", "COPYING"),
    "genesis_plus_gx": ("ekeeke/Genesis-Plus-GX", "27426f00aa68f9f358c86919e8a40985326fa05b", "LICENSE.txt"),
}

# The other Apple Silicon cores are downloads from the official libretro
# macOS arm64 buildbot. We keep each downloaded archive and record its hash in
# the kit, together with a pinned source snapshot for provenance and the full
# licence notice. That snapshot is not the source revision of the nightly
# binary.
PREBUILT_CORES = {
    "mgba": {
        "binary": "mgba_libretro.dylib",
        "repo": "mgba-emu/mgba",
        "revision": "3a5bc24629867576b0fb576a5d5a21d3b3d6b576",
        "licenses": ("LICENSE",),
        "license": "MPL-2.0",
    },
    "nestopia": {
        "binary": "nestopia_libretro.dylib",
        "repo": "libretro/nestopia",
        "revision": "92578fdc9445f61dd376138329a938e01d8ba50e",
        "licenses": ("COPYING", "LICENSE"),
        "license": "GPL-2.0",
    },
    "snes9x": {
        "binary": "snes9x_libretro.dylib",
        "repo": "snes9xgit/snes9x",
        "revision": "4998efce010c95c1d83859a0530cdf43d2d27cb9",
        "licenses": ("LICENSE", "docs/snes9x-license.txt"),
        "license": "Snes9x non-commercial",
    },
    "beetle_pce_fast": {
        "binary": "mednafen_pce_fast_libretro.dylib",
        "repo": "libretro/beetle-pce-fast-libretro",
        "revision": "076a24e1b10f76f3a7a8e849d25fab89f81ce1e9",
        "licenses": ("COPYING", "LICENSE"),
        "license": "GPL-2.0",
    },
    "stella": {
        "binary": "stella_libretro.dylib",
        "repo": "libretro/stella2014-libretro",
        "revision": "7d1361e407e63f29e52892655069e5fb4096e691",
        "licenses": ("stella/license.txt", "License.txt", "LICENSE", "COPYING"),
        "license": "GPL-2.0",
    },
}

BUILDBOT_BASE = "https://buildbot.libretro.com/nightly/apple/osx/arm64/latest"


def download(url: str, path: Path) -> None:
    """Cache an upstream source artifact without repeatedly downloading it."""
    if path.exists():
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url, timeout=90) as response, path.open("wb") as output:
        shutil.copyfileobj(response, output)


def digest(path: Path) -> str:
    """Record the exact bytes used for this local runtime kit."""
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def copy_license_from_source(archive: Path, candidates: tuple[str, ...], destination: Path) -> str:
    """Extract a declared license from a pinned GitHub source snapshot."""
    with tarfile.open(archive) as package:
        files = [member for member in package.getmembers() if member.isfile()]
        for candidate in candidates:
            suffix = f"/{candidate}"
            member = next((item for item in files if item.name.endswith(suffix)), None)
            if member is None:
                continue
            source = package.extractfile(member)
            if source is None:
                continue
            destination.write_bytes(source.read())
            return member.name
    raise RuntimeError(f"No declared license {candidates!r} in {archive}")


def prepare_prebuilt_core(root: Path, component: str, spec: dict[str, object]) -> dict[str, object]:
    """Stage one official arm64 buildbot core without executing it."""
    binary_name = str(spec["binary"])
    binary_archive = root / "sources" / f"buildbot-{binary_name}.zip"
    binary_url = f"{BUILDBOT_BASE}/{binary_name}.zip"
    download(binary_url, binary_archive)
    with zipfile.ZipFile(binary_archive) as package:
        members = [name for name in package.namelist() if not name.endswith("/")]
        if members != [binary_name]:
            raise RuntimeError(f"Unexpected files in {binary_archive}: {members}")
        binary = root / "cores" / binary_name
        with package.open(binary_name) as source, binary.open("wb") as output:
            shutil.copyfileobj(source, output)

    architecture = subprocess.run(
        ["/usr/bin/lipo", "-archs", str(binary)],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.split()
    if "arm64" not in architecture:
        raise RuntimeError(f"Buildbot core is not arm64: {binary}: {architecture}")

    repo = str(spec["repo"])
    revision = str(spec["revision"])
    source_archive = root / "sources" / f"{component}-{revision}.tar.gz"
    source_url = f"https://codeload.github.com/{repo}/tar.gz/{revision}"
    download(source_url, source_archive)
    license_member = copy_license_from_source(
        source_archive,
        tuple(spec["licenses"]),
        root / "licenses" / f"{component}.txt",
    )
    return {
        "name": component,
        "binary": binary_name,
        "binary_origin": "Official Libretro macOS arm64 nightly buildbot",
        "binary_url": binary_url,
        "binary_archive": binary_archive.name,
        "binary_archive_sha256": digest(binary_archive),
        "binary_sha256": digest(binary),
        "architecture": architecture,
        "license": spec["license"],
        "license_source_member": license_member,
        "source_notice_revision": revision,
        "source_url": source_url,
        "source_archive": source_archive.name,
        "source_sha256": digest(source_archive),
    }


def main() -> None:
    """Build native macOS cores from pinned sources and prepare the local prototype kit."""
    parser = argparse.ArgumentParser(description=main.__doc__)
    parser.add_argument("--retroarch", type=Path, default=Path("/Applications/RetroArch.app"))
    parser.add_argument("--output", type=Path, default=Path("work/runtime-kit"))
    parser.add_argument(
        "--include-official-arm64-cores",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="include the declared GBA, NES, SNES, PC Engine, and Atari 2600 cores from Libretro's official arm64 buildbot",
    )
    parser.add_argument(
        "--official-arm64-cores-only",
        action="store_true",
        help="prepare only the additional official arm64 cores, licenses, sources, and components.json in a staging directory",
    )
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("This preparation recipe currently builds the macOS prototype kit.")
    root = args.output.resolve()
    for name in ("cores", "sources", "licenses", "catalogs", "info"):
        (root / name).mkdir(parents=True, exist_ok=True)
    entries = []
    if args.official_arm64_cores_only:
        for component, spec in PREBUILT_CORES.items():
            print(f"Preparing {component} from official arm64 buildbot", flush=True)
            entries.append(prepare_prebuilt_core(root, component, spec))
        (root / "components.json").write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "platform": "darwin",
                    "architecture": platform.machine(),
                    "components": entries,
                },
                indent=2,
            )
        )
        print(f"Additional core staging ready: {root}", flush=True)
        return
    for name, (repo, revision, license_file) in CORES.items():
        archive = root / "sources" / f"{name}-{revision}.tar.gz"
        url = f"https://codeload.github.com/{repo}/tar.gz/{revision}"
        print(f"Preparing {name}", flush=True)
        download(url, archive)
        build_parent = root.parent / "core-builds" / name
        build_parent.mkdir(parents=True, exist_ok=True)
        if not list(build_parent.iterdir()):
            with tarfile.open(archive) as package:
                package.extractall(build_parent, filter="data")
        source = next(build_parent.iterdir())
        recipe = [
            "make",
            "-f",
            "Makefile.libretro",
            "-j4",
            "platform=osx",
            f"ARCHFLAGS=-arch {platform.machine()}",
            "HAVE_CHD=0",
            f"GIT_VERSION= {revision[:8]}",
        ]
        with (root.parent / f"build-{name}.log").open("w") as log:
            subprocess.run(recipe, cwd=source, stdout=log, stderr=subprocess.STDOUT, check=True)
        binary = root / "cores" / f"{name}_libretro.dylib"
        shutil.copy2(source / binary.name, binary)
        subprocess.run(["codesign", "--force", "--sign", "-", str(binary)], check=True)
        shutil.copy2(source / license_file, root / "licenses" / f"{name}.txt")
        (root / "info" / f"{name}_libretro.info").write_text(
            f'display_name = "{name}"\nsavestate = "true"\nsavestate_features = "serialized"\n'
        )
        entries.append(
            {
                "name": name,
                "revision": revision,
                "source_url": url,
                "source_archive": archive.name,
                "source_sha256": digest(archive),
                "binary_sha256": digest(binary),
                "build_command": recipe,
                "architecture": platform.machine(),
                "license": "GPL-2.0-or-later" if name == "gambatte" else "Genesis Plus GX non-commercial",
            }
        )
    if args.include_official_arm64_cores:
        for component, spec in PREBUILT_CORES.items():
            print(f"Preparing {component} from official arm64 buildbot", flush=True)
            entries.append(prepare_prebuilt_core(root, component, spec))
    if not (root / "RetroArch.app").exists():
        shutil.copytree(args.retroarch, root / "RetroArch.app", symlinks=True)
    ra_repo = "https://raw.githubusercontent.com/libretro/RetroArch/e33bb934/"
    download(ra_repo + "COPYING", root / "licenses/RetroArch.txt")
    download(
        "https://codeload.github.com/libretro/RetroArch/tar.gz/e33bb934", root / "sources/RetroArch-e33bb934.tar.gz"
    )
    entries.append(
        {
            "name": "RetroArch",
            "revision": "e33bb934",
            "version": "1.22.2",
            "source_archive": "RetroArch-e33bb934.tar.gz",
            "source_sha256": digest(root / "sources/RetroArch-e33bb934.tar.gz"),
            "origin": "Locally installed upstream macOS application; original bundle preserved",
            "binary_sha256": digest(root / "RetroArch.app/Contents/MacOS/RetroArch"),
            "license": "GPL-3.0",
            "source_url": "https://github.com/libretro/RetroArch/tree/e33bb934",
        }
    )
    # Catalogs are optional. Record their actual revision and digest for this kit.
    with urllib.request.urlopen(
        "https://api.github.com/repos/libretro/libretro-database/commits/master", timeout=20
    ) as response:
        revision = json.load(response)["sha"]
    for catalog in ("Nintendo - Game Boy", "Nintendo - Game Boy Color", "Sega - Mega Drive - Genesis"):
        url = f"https://raw.githubusercontent.com/libretro/libretro-database/{revision}/metadat/no-intro/{urllib.parse.quote(catalog)}.dat"
        try:
            download(url, root / "catalogs" / f"{catalog}.dat")
        except OSError as exc:
            print(f"Optional catalog unavailable: {catalog}: {exc}", flush=True)
    (root / "components.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "platform": "darwin",
                "architecture": platform.machine(),
                "components": entries,
                "catalog_revision": revision,
            },
            indent=2,
        )
    )
    (root / "licenses/README.txt").write_text(
        "Local prototype runtime. Component licenses and pinned core source archives accompany this kit.\n"
        "Genesis Plus GX is restricted to non-commercial use/distribution.\n"
        "Do not distribute game ROMs, firmware, or artwork without the necessary rights.\n"
        "A public release still requires a complete bundled-library and source-completeness audit.\n"
    )
    print(f"Runtime kit ready: {root}", flush=True)


if __name__ == "__main__":
    main()
