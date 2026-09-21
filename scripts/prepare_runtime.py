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

# The provenance of each core is declared in the console package that
# contains its component. We read it from the catalog here instead of keeping
# a second table that could differ from it.
#
# We use `correspondsToArtifact` to tell the two kinds of core apart. With a
# core we build ourselves, we include the exact source we built it from. With
# a buildbot download, we keep a source snapshot only for its licence text,
# and that snapshot is not the corresponding source of the GPL binary.
CATALOG_MANIFEST = Path(__file__).resolve().parent.parent / "desktop/crates/rominabox-catalog/Cargo.toml"


def catalog_components() -> dict[str, dict]:
    """Ask the catalog for every declared core component."""
    result = subprocess.run(
        [
            "cargo", "run", "--quiet",
            "--manifest-path", str(CATALOG_MANIFEST),
            "--bin", "rominabox-catalog", "--", "components",
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    return {entry["id"]: entry for entry in json.loads(result.stdout)}


def partitioned_components(target: str) -> tuple[dict, dict]:
    """Split declared components into ones we build and ones we download.

    We skip a component with no artifact declared for this target, because
    we cannot know what its file would be called.
    """
    built, prebuilt = {}, {}
    for cid, entry in catalog_components().items():
        provenance = entry.get("provenance")
        if not provenance:
            continue
        if not entry["artifacts"].get(target):
            continue
        binary = entry["artifacts"].get(target)
        record = {
            "binary": binary,
            "repo": provenance["repository"],
            "revision": provenance["revision"],
            "licenses": tuple(provenance["licenseCandidates"]),
            "license": entry["license"]["spdx"],
            "corresponds_to_artifact": provenance["correspondsToArtifact"],
            "build": provenance.get("build") or {},
        }
        if provenance["origin"] == "built":
            built[cid] = record
        else:
            prebuilt[cid] = record
    return built, prebuilt


# The address of the official nightly build for each target. We prepare a kit
# for a target given as an argument, not for the machine we run on, so we make
# a Windows kit with the same script and a different argument.
BUILDBOT_PATHS = {
    "macos-arm64": "apple/osx/arm64",
    "macos-x86_64": "apple/osx/x86_64",
    "windows-x86_64": "windows/x86_64",
    "windows-arm64": "windows/arm64",
    "linux-x86_64": "linux/x86_64",
}


def buildbot_base(target: str) -> str:
    try:
        return f"https://buildbot.libretro.com/nightly/{BUILDBOT_PATHS[target]}/latest"
    except KeyError:
        raise SystemExit(
            f"No official nightly path is known for {target}; "
            f"known targets: {', '.join(sorted(BUILDBOT_PATHS))}"
        ) from None


def host_target() -> str:
    """The default target for builds on this machine."""
    machine = platform.machine()
    system = platform.system()
    architecture = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x86_64", "AMD64": "x86_64"}
    if system == "Darwin":
        return f"macos-{architecture.get(machine, machine)}"
    if system == "Windows":
        return f"windows-{architecture.get(machine, machine)}"
    return f"linux-{architecture.get(machine, machine)}"


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


def prepare_prebuilt_core(root: Path, component: str, spec: dict[str, object], target: str) -> dict[str, object]:
    """Stage one official buildbot core for a target, without executing it."""
    binary_name = str(spec["binary"])
    binary_archive = root / "sources" / f"buildbot-{binary_name}.zip"
    binary_url = f"{buildbot_base(target)}/{binary_name}.zip"
    download(binary_url, binary_archive)
    with zipfile.ZipFile(binary_archive) as package:
        members = [name for name in package.namelist() if not name.endswith("/")]
        if members != [binary_name]:
            raise RuntimeError(f"Unexpected files in {binary_archive}: {members}")
        binary = root / "cores" / binary_name
        with package.open(binary_name) as source, binary.open("wb") as output:
            shutil.copyfileobj(source, output)

    # lipo exists only on macOS. Other targets require their own check, and we
    # must not accept them without one.
    if target.startswith("macos"):
        architecture = subprocess.run(
            ["/usr/bin/lipo", "-archs", str(binary)],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.split()
    else:
        raise SystemExit(
            f"Preparing {target} needs an architecture check for that platform; "
            "refusing to stage a core whose architecture was never verified."
        )
    expected = target.rsplit("-", 1)[1]
    if expected not in architecture:
        raise RuntimeError(
            f"Buildbot core for {target} is not {expected}: {binary}: {architecture}"
        )

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
        "binary_origin": f"Official Libretro {target} nightly buildbot",
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
    parser.add_argument(
        "--target",
        help="Target to prepare, e.g. macos-arm64 or windows-x86_64. Defaults to this machine.",
    )
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
    target = args.target or host_target()
    built_cores, prebuilt_cores = partitioned_components(target)
    if args.official_arm64_cores_only:
        for component, spec in prebuilt_cores.items():
            print(f"Preparing {component} from official arm64 buildbot", flush=True)
            entries.append(prepare_prebuilt_core(root, component, spec, target))
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
    for name, spec in built_cores.items():
        repo, revision = spec["repo"], spec["revision"]
        license_file = spec["licenses"][0]
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
        # We take the recipe from the component. What the core can do depends on
        # its build flags, so a component that requires CHD declares that flag
        # itself, and we keep no constant for it in this file.
        build = spec.get("build") or {}
        recipe = [
            "make",
            "-f",
            build.get("makefile", "Makefile.libretro"),
            "-j4",
            f"platform={build.get('platform', 'osx')}",
            f"ARCHFLAGS=-arch {platform.machine()}",
            *build.get("flags", []),
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
        for component, spec in prebuilt_cores.items():
            print(f"Preparing {component} from official arm64 buildbot", flush=True)
            entries.append(prepare_prebuilt_core(root, component, spec, target))
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
