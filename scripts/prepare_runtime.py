import argparse
import hashlib
import json
import platform
import re
import shutil
import subprocess
import tarfile
import urllib.parse
import urllib.request
import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from core_source import DOWNLOADS, host_target, seeded_cache  # noqa: E402
import joypad_autoconfig  # noqa: E402
import licences  # noqa: E402
import native_build  # noqa: E402

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
            "downloads": provenance.get("downloads") or {},
        }
        # A recipe named osx compiles for this machine. With it, we would give
        # a Windows or Linux target the host executable under that platform's
        # file name, so for those targets we use the pinned nightly.
        if provenance["origin"] == "built" and recipe_covers(record, target):
            built[cid] = record
        elif record["downloads"].get(target):
            if provenance["origin"] == "built":
                record["corresponds_to_artifact"] = False
            prebuilt[cid] = record
        elif provenance["origin"] != "built":
            prebuilt[cid] = record
    return built, prebuilt


def recipe_covers(record: dict, target: str) -> bool:
    """Return whether we can build `target` here with the component's recipe."""
    platform_name = (record.get("build") or {}).get("platform")
    if platform_name != "osx":
        return False
    return target == host_target() and target.startswith("macos")


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


# The builder's kit. It contains no cores and no source archives, because we
# download a core at export. We refuse to stage a core into this kit.
BUNDLED_KIT = Path(__file__).resolve().parent.parent / "desktop/src-tauri/resources/runtime"


def refuse_bundled_kit(root: Path) -> None:
    if root == BUNDLED_KIT.resolve() or BUNDLED_KIT.resolve() in root.parents:
        raise SystemExit(
            f"{root} is the builder's runtime kit, which bundles no cores. "
            "Use --seed-core-cache for a local core source."
        )


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


def prepare_prebuilt_core(
    root: Path, component: str, spec: dict[str, object], target: str, downloads: Path | None = None
) -> dict[str, object]:
    """Stage one official buildbot core for a target, without executing it.

    `downloads` is the folder where we keep the downloaded archives, and
    `root/sources` when none is given.
    """
    downloads = downloads or root / "sources"
    binary_name = str(spec["binary"])
    binary_archive = downloads / f"buildbot-{binary_name}.zip"
    binary_url = f"{buildbot_base(target)}/{binary_name}.zip"
    download(binary_url, binary_archive)
    with zipfile.ZipFile(binary_archive) as package:
        members = [name for name in package.namelist() if not name.endswith("/")]
        if members != [binary_name]:
            raise RuntimeError(f"Unexpected files in {binary_archive}: {members}")
        binary = root / "cores" / binary_name
        with package.open(binary_name) as source, binary.open("wb") as output:
            shutil.copyfileobj(source, output)

    downloads = spec.get("downloads") or {}
    pinned = downloads.get(target) or {}
    # libretro replaces the file at `latest` in place. The hash recorded on the
    # component identifies one nightly, and a file with another hash is a different core.
    expected_archive = pinned.get("archiveSha256")
    if expected_archive and digest(binary_archive) != expected_archive:
        raise RuntimeError(
            f"{component} archive for {target} is not the recorded nightly"
        )
    expected_binary = pinned.get("binarySha256")
    if expected_binary and digest(binary) != expected_binary:
        raise RuntimeError(
            f"{component} binary for {target} is not the recorded nightly"
        )

    # lipo exists only on macOS. We accept a core for another target when its
    # hash matches the recorded one, because then the bytes are the ones whose
    # architecture we read when we recorded the hash. Without a recorded hash
    # we have nothing to trust, and we do not stage the core.
    if target.startswith("macos"):
        architecture = subprocess.run(
            ["/usr/bin/lipo", "-archs", str(binary)],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.split()
        expected = target.rsplit("-", 1)[1]
        if expected not in architecture:
            raise RuntimeError(
                f"Buildbot core for {target} is not {expected}: {binary}: {architecture}"
            )
    elif expected_binary:
        architecture = [target.rsplit("-", 1)[1]]
    else:
        raise SystemExit(
            f"Preparing {target} needs a recorded hash for that platform; "
            "refusing to stage a core whose bytes were never verified."
        )

    repo = str(spec["repo"])
    revision = str(spec["revision"])
    source_archive = downloads / f"{component}-{revision}.tar.gz"
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


def build_core(root: Path, name: str, spec: dict, downloads: Path) -> dict[str, object]:
    """Compile one core from its pinned source into `root/cores`, with its licence."""
    repo, revision = spec["repo"], spec["revision"]
    license_file = spec["licenses"][0]
    archive = downloads / f"{name}-{revision}.tar.gz"
    url = f"https://codeload.github.com/{repo}/tar.gz/{revision}"
    print(f"Preparing {name}", flush=True)
    download(url, archive)
    work = DOWNLOADS.parent
    build_parent = work / "core-builds" / name
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
    with (work / f"build-{name}.log").open("w") as log:
        subprocess.run(recipe, cwd=source, stdout=log, stderr=subprocess.STDOUT, check=True)
    binary = root / "cores" / f"{name}_libretro.dylib"
    shutil.copy2(source / binary.name, binary)
    subprocess.run(["codesign", "--force", "--sign", "-", str(binary)], check=True)
    shutil.copy2(source / license_file, root / "licenses" / f"{name}.txt")
    if (root / "info").is_dir():
        (root / "info" / f"{name}_libretro.info").write_text(
            f'display_name = "{name}"\nsavestate = "true"\nsavestate_features = "serialized"\n'
        )
    return (
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


# Pinned joypad profiles. In RetroArch, profiles come only from
# joypad_autoconfig_dir and joypad_autoconfig_dir/<driver>, one level deep,
# so we stage only the driver folders declared for the kit's platform
# (player-recipe.json, "drivers"). SDL3 gamecontrollerdb.cfg is not a profile.
JOYPAD_AUTOCONFIG_REPO = "libretro/retroarch-joypad-autoconfig"
JOYPAD_AUTOCONFIG_REVISION = "1c6d74cef79b56a3a5dc283b1b0b2e4af73376ff"
JOYPAD_AUTOCONFIG_LICENSE_FILE = "retroarch-joypad-autoconfig.txt"
JOYPAD_AUTOCONFIG_COMPONENT = "retroarch-joypad-autoconfig"

# We test with a DualSense (1356/3302), so under each driver where a pad is
# identified by its ids, this profile must be the only one that matches it. In
# XInput every pad is called "XInput Controller", and a DualSense never appears.
DUALSENSE_PROFILES = {
    "hid": "DualSense Wireless Controller (PS5).cfg",
    "dinput": "DualSense5.cfg",
}

_PLAYER_PREFIX = re.compile(r"^player\d+_")
_ALT_SUFFIX = re.compile(r"_alt\d+$")
_META_BIND_SUFFIXES = ("_btn_label", "_axis_label", "_btn", "_axis", "_mbtn")


def meta_bind_names(configuration_c: Path | None = None) -> list[str]:
    """Names declared by `DECLARE_META_BIND` in the pinned RetroArch sources.

    We read the list from the source tree instead of copying it, so we strip
    a new meta action the next time we stage the profiles.
    """
    path = configuration_c or (
        Path(__file__).resolve().parent.parent / "vendor/retroarch/configuration.c"
    )
    names: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped.startswith("DECLARE_META_BIND("):
            continue
        name = stripped.split(",")[1].strip()
        if name not in names:
            names.append(name)
    if not names:
        raise RuntimeError(f"No DECLARE_META_BIND names in {path}")
    return names


def is_meta_bind_key(key: str, names: set[str]) -> bool:
    """Return True when an autoconfig key assigns a meta/hotkey action.

    In RetroArch, `input_<name>_btn`, `input_<name>_axis` and their `_label`
    keys all apply to the autoconfig bind. Player prefixes and `_altN`
    alternatives refer to the same bind. We also remove a keyboard
    `input_<name>` or `_mbtn` line, so that a line we left behind cannot
    become a hotkey in a later version of the parser.
    """
    if not key.startswith("input_"):
        return False
    rest = _ALT_SUFFIX.sub("", _PLAYER_PREFIX.sub("", key[len("input_") :], count=1))
    for name in names:
        if rest == name:
            return True
        for suffix in _META_BIND_SUFFIXES:
            if rest == f"{name}{suffix}":
                return True
    return False


# The keys a joypad profile may contain. They describe the device and how its
# buttons and axes map to the sixteen gameplay binds. We drop every other key.
#
# The `_altN` forms describe the device too. In RetroArch
# (`task_autodetect.c`), `input_device_alt1` and `input_vendor_id_alt1` count
# in the same pass as the main entry, and `input_phys` counts only after a
# name or id match. Without any of these keys, a pad the profile was written
# for scores zero, and the log shows "not configured".
DEVICE_KEYS = {
    "input_device",
    "input_device_display_name",
    "input_driver",
    "input_vendor_id",
    "input_product_id",
    "input_phys",
}
GAMEPLAY_BINDS = {
    "up", "down", "left", "right", "a", "b", "x", "y",
    "l", "r", "l2", "r2", "l3", "r3", "start", "select",
    "l_x_plus", "l_x_minus", "l_y_plus", "l_y_minus",
    "r_x_plus", "r_x_minus", "r_y_plus", "r_y_minus",
}
# We match the compound forms first. `input_b_btn_label` is a gameplay label,
# and removing only `_label` would leave `b_btn`, which is not a bind name.
BIND_SUFFIXES = (
    "_btn_label",
    "_axis_label",
    "_mbtn_label",
    "_btn",
    "_axis",
    "_mbtn",
    "_label",
)


def home_button_key(inc: Path | None = None) -> str:
    """Return the one meta line we keep in a profile. It sets the pad's own
    menu button, which is Home in the player's MENU CONTROLS. We read that
    bind from the player's declaration (`RIB_MENU_PAD_HOME` in
    menu_controls.inc) instead of copying it."""
    path = inc or (
        Path(__file__).resolve().parent.parent
        / "vendor/retroarch/menu/drivers/rmlui/menu_controls.inc"
    )
    for line in path.read_text(encoding="utf-8").splitlines():
        found = re.match(r'\s*RIB_MENU_PAD_HOME\("[^"]*",\s*"([^"]+)"', line)
        if found:
            return f"input_{found.group(1)}_btn"
    raise RuntimeError(f"No RIB_MENU_PAD_HOME in {path}")


def is_allowed_key(key: str) -> bool:
    """Return whether a profile may contain this assignment."""
    if _ALT_SUFFIX.sub("", key) in DEVICE_KEYS:
        return True
    # The pad's own menu button, as a button only. It does not open the
    # RetroArch menu, and in our menu it is Home.
    if key == home_button_key():
        return True
    if not key.startswith("input_"):
        return False
    rest = key[len("input_"):]
    for suffix in BIND_SUFFIXES:
        if rest.endswith(suffix):
            rest = rest[: -len(suffix)]
            break
    return rest in GAMEPLAY_BINDS


def strip_meta_bind_lines(text: str, names: set[str]) -> tuple[str, int]:
    """Keep only the lines a joypad profile may contain, and drop the rest.

    In RetroArch, a profile can set any bind in the bind table
    (`input_config_set_autoconfig_binds`, `configuration.c:7529-7540`), and
    that table is wider than the DECLARE_META_BIND names, because `turbo` and
    `hold` are in it without being meta binds. With a list of meta binds to
    remove, a profile that binds `input_turbo_btn` would keep that line, and
    the pad would have turbo fire, so we keep a list of allowed keys instead.

    We also drop `#` lines. In RetroArch, `#include` includes a file
    (`config_file.c:159-175`), and we never see the lines of an included file
    in this function, so a line that looks like a comment could bring back
    the hotkey we remove here.

    `names` is unused, and we keep it so that the callers stay the same.
    """
    del names
    kept: list[str] = []
    removed = 0
    for line in text.splitlines(keepends=True):
        body = line.strip()
        if not body:
            kept.append(line)
            continue
        if body.startswith("#"):
            # A plain comment is harmless and an include is not. We do not try
            # to tell them apart, and drop both.
            if body.lower().startswith("#include"):
                removed += 1
                continue
            kept.append(line)
            continue
        if "=" not in line:
            removed += 1
            continue
        if is_allowed_key(line.split("=", 1)[0].strip()):
            kept.append(line)
            continue
        removed += 1
    return "".join(kept), removed


def _profile(member_name: str, drivers: list[str]) -> tuple[str, str] | None:
    """Return (driver, cfg basename) when the member is exactly
    `<root>/<driver>/<file>.cfg` for one of `drivers`."""
    parts = Path(member_name).parts
    if len(parts) != 3 or parts[1] not in drivers:
        return None
    filename = parts[2]
    if not filename.endswith(".cfg") or filename.startswith("."):
        return None
    return parts[1], filename


def _record_joypad_component(root: Path, record: dict[str, object]) -> None:
    """Add or update the component in an existing kit manifest, as we do for cores."""
    manifest_path = root / "manifest.json"
    if not manifest_path.is_file():
        return
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    components = manifest.get("components")
    if not isinstance(components, list):
        raise RuntimeError(f"{manifest_path} has no components array")
    components = [item for item in components if item.get("name") != JOYPAD_AUTOCONFIG_COMPONENT]
    components.append(record)
    manifest["components"] = components
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def stage_joypad_autoconfig(root: Path, drivers: list[str]) -> dict[str, object]:
    """Download the pinned autoconfig repository and stage the profiles of `drivers`.

    Keep the downloaded archive in work/downloads. For export, stage only
    `<driver>/*.cfg` for each of `drivers`, without the meta/hotkey
    assignment lines. Beside the other component licences, store the
    repository's licence entry for the profiles (licenses/data), which must
    contain this archive's `COPYING`.
    """
    if not drivers:
        raise RuntimeError("no joypad drivers to stage profiles for")
    revision = JOYPAD_AUTOCONFIG_REVISION
    names = set(meta_bind_names())
    archive = DOWNLOADS / f"{JOYPAD_AUTOCONFIG_COMPONENT}-{revision}.tar.gz"
    source_url = f"https://codeload.github.com/{JOYPAD_AUTOCONFIG_REPO}/tar.gz/{revision}"
    download(source_url, archive)
    licence_member = None
    profiles: list[tuple[str, str, bytes]] = []
    with tarfile.open(archive) as package:
        for member in package.getmembers():
            if not member.isfile():
                continue
            profile = _profile(member.name, drivers)
            if profile is None:
                if Path(member.name).name == "COPYING" and len(Path(member.name).parts) == 2:
                    source = package.extractfile(member)
                    if source is None:
                        continue
                    licence_member = member.name
                    entry = licences.OUT / "data" / f"{JOYPAD_AUTOCONFIG_COMPONENT}.txt"
                    if licences.clean(licences.sources.decode(source.read())) not in licences.sections(entry).values():
                        raise RuntimeError(f"{entry} does not hold {licence_member}; run scripts/licences.py")
                    destination = root / "licenses" / JOYPAD_AUTOCONFIG_LICENSE_FILE
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(entry, destination)
                continue
            source = package.extractfile(member)
            if source is None:
                continue
            profiles.append((*profile, source.read()))
    if licence_member is None:
        raise RuntimeError(f"No COPYING member in {archive}")
    licence_text = (root / "licenses" / JOYPAD_AUTOCONFIG_LICENSE_FILE).read_text(encoding="utf-8")
    if "Copyright (c) 2019 The RetroArch team" not in licence_text:
        raise RuntimeError("Joypad autoconfig COPYING is missing the RetroArch MIT copyright")
    if "Permission is hereby granted" not in licence_text:
        raise RuntimeError("Joypad autoconfig COPYING is missing the MIT grant")
    # When upstream disabled a profile by commenting out its device name or an
    # id, no pad can match it in RetroArch, so we leave it out of the kit.
    disabled = [(driver, filename) for driver, filename, raw in profiles
                if not joypad_autoconfig.can_match(joypad_autoconfig.parse_entries(raw.decode("utf-8")))]
    profiles = [profile for profile in profiles if (profile[0], profile[1]) not in disabled]
    for driver in drivers:
        if not any(staged == driver for staged, _filename, _raw in profiles):
            raise RuntimeError(f"No {driver} profiles in {archive}")

    for driver in drivers:
        destination_dir = root / "autoconfig" / driver
        destination_dir.mkdir(parents=True, exist_ok=True)
        expected = {filename for staged, filename, _raw in profiles if staged == driver}
        for child in destination_dir.iterdir():
            if child.is_file() and child.name not in expected:
                child.unlink()

    removed_lines = 0
    upstream_bytes = 0
    staged_bytes = 0
    dualsense_files: dict[str, list[str]] = {driver: [] for driver in drivers}
    for driver, filename, raw in profiles:
        upstream_bytes += len(raw)
        text = raw.decode("utf-8")
        stripped, removed = strip_meta_bind_lines(text, names)
        removed_lines += removed
        if any(
            is_meta_bind_key(line.split("=", 1)[0].strip(), names)
            and line.split("=", 1)[0].strip() != home_button_key()
            for line in stripped.splitlines()
            if "=" in line and not line.strip().startswith("#")
        ):
            raise RuntimeError(f"meta bind survived staging in {filename}")
        payload = stripped.encode("utf-8")
        (root / "autoconfig" / driver / filename).write_bytes(payload)
        staged_bytes += len(payload)
        if 'input_vendor_id = "1356"' in stripped and 'input_product_id = "3302"' in stripped:
            dualsense_files[driver].append(filename)
    for driver in drivers:
        expected_dualsense = [DUALSENSE_PROFILES[driver]] if driver in DUALSENSE_PROFILES else []
        if dualsense_files[driver] != expected_dualsense:
            raise RuntimeError(
                f"expected the DualSense 1356/3302 profiles {expected_dualsense} under {driver}, "
                f"found {dualsense_files[driver]}"
            )

    record: dict[str, object] = {
        "name": JOYPAD_AUTOCONFIG_COMPONENT,
        "revision": revision,
        "source_url": f"https://github.com/{JOYPAD_AUTOCONFIG_REPO}/tree/{revision}",
        "license": "MIT",
        "license_file": JOYPAD_AUTOCONFIG_LICENSE_FILE,
        "license_source_member": licence_member,
        "origin": (
            f"Pinned {', '.join(drivers)} profiles from libretro/retroarch-joypad-autoconfig. "
            f"Only {', '.join(f'{driver}/*.cfg' for driver in drivers)} is staged: the folders "
            "the exported player's controller driver reads. Profiles upstream disabled, which "
            "name no device and no complete pair of ids, are not staged. "
            "Meta-bind lines are removed at staging, but for the pad's own menu button, which "
            "the menu reads as Home. SDL3 gamecontrollerdb.cfg is not shipped."
        ),
    }
    _record_joypad_component(root, record)
    print(
        f"Joypad autoconfig {revision}: {len(profiles)} {'/'.join(drivers)} profiles, "
        f"{len(disabled)} disabled ones left out, {removed_lines} meta lines removed, {staged_bytes} staged bytes "
        f"(upstream {upstream_bytes} bytes, archive {archive.stat().st_size} bytes)",
        flush=True,
    )
    return record


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
    parser.add_argument(
        "--seed-core-cache",
        action="store_true",
        help="compile or download this target's cores and their licences into work/core-cache/<target>, "
        "the local core source tests read; nothing else is prepared",
    )
    parser.add_argument(
        "--joypad-autoconfig-only",
        action="store_true",
        help="stage the pinned joypad profiles the target's platform declares, their COPYING, "
        "and the manifest record into --output",
    )
    args = parser.parse_args()
    if args.joypad_autoconfig_only:
        root = args.output.resolve()
        (root / "sources").mkdir(parents=True, exist_ok=True)
        (root / "licenses").mkdir(parents=True, exist_ok=True)
        platform_name = (args.target or host_target()).split("-", 1)[0]
        stage_joypad_autoconfig(root, native_build.joypad_profile_drivers(platform_name))
        print(f"Joypad autoconfig staged: {root}", flush=True)
        return
    target = args.target or host_target()
    # For seeding we take the target as an argument and download what we cannot
    # build here, so seeding works on any machine. The kit below is for macOS.
    if args.seed_core_cache:
        root = seeded_cache(target)
        for name in ("cores", "licenses"):
            (root / name).mkdir(parents=True, exist_ok=True)
        built_cores, prebuilt_cores = partitioned_components(target)
        for name, spec in built_cores.items():
            build_core(root, name, spec, DOWNLOADS)
        for component, spec in prebuilt_cores.items():
            print(f"Preparing {component} from the official buildbot", flush=True)
            prepare_prebuilt_core(root, component, spec, target, DOWNLOADS)
        print(f"Local core source ready: {root}", flush=True)
        return
    if platform.system() != "Darwin":
        parser.error("This preparation recipe currently builds the macOS prototype kit.")
    root = args.output.resolve()
    refuse_bundled_kit(root)
    for name in ("cores", "sources", "licenses", "catalogs", "info"):
        (root / name).mkdir(parents=True, exist_ok=True)
    entries = []
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
        entries.append(build_core(root, name, spec, root / "sources"))
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
    entries.append(stage_joypad_autoconfig(root, native_build.joypad_profile_drivers(target.split("-", 1)[0])))
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
