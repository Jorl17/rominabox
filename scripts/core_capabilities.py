"""Read the content formats listed in a built core.

The capabilities of a core depend on the binary, not on the upstream project.
That a codebase supports CHD does not show that the binary we use was compiled
with CHD support, so we read the answer out of the binary and base each
declaration on it.

Every libretro core contains its `valid_extensions` string, a pipe-separated
list such as `bin|cue|chd|iso`. In the frontend we pass the core only files
with those extensions. In a compiled core, that string is the closest thing
to a machine-readable list of capabilities.

    python3 scripts/core_capabilities.py CORE [CORE ...]
    python3 scripts/core_capabilities.py --check      # every core in the local core source

Reading a string is weaker evidence than loading the core and asking it, but
it is far stronger than trusting a project's reputation, and we do not have
to run a downloaded binary.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from core_source import core_source  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
# We bundle no cores with the builder, so we use those in the local core source.
KIT_CORES = core_source() / "cores"
CATALOG_MANIFEST = ROOT / "desktop/crates/rominabox-catalog/Cargo.toml"

# We declare only formats whose support differs between builds. Nobody needs
# to check that a cartridge core can read a .md.
CONTAINER_FORMATS = {"chd", "cue", "iso", "gdi", "cdi", "pbp", "rvz", "m3u"}

# A pipe-separated run of short lowercase extensions, which is how
# valid_extensions is stored.
EXTENSION_LIST = re.compile(r"^[a-z0-9]{1,5}(\|[a-z0-9]{1,5}){2,}$")


# Listing an extension is not the same as being able to read it. Genesis Plus
# GX and Beetle PCE list `chd` in valid_extensions but contain no libchdr,
# because the extension list is always compiled in and the decoder is not. So
# for formats with a separate library we confirm support by symbols.
LIBRARY_EVIDENCE = {"chd": re.compile(r" [TtDdSs] _?(chd_|CHDR_)", re.I)}


def links_library(core: Path, extension: str) -> bool:
    """Whether the decoder for a format is actually present in the binary."""
    pattern = LIBRARY_EVIDENCE.get(extension)
    if pattern is None:
        return True
    symbols = subprocess.run(
        ["nm", "-a", str(core)], capture_output=True, text=True
    ).stdout
    return any(pattern.search(line) for line in symbols.splitlines())


def supported_containers(core: Path) -> set[str]:
    """Container formats that this build lists and can also decode."""
    return {
        extension
        for extension in advertised_extensions(core) & CONTAINER_FORMATS
        if links_library(core, extension)
    }


def advertised_extensions(core: Path) -> set[str]:
    """Every extension listed in the built core."""
    strings = subprocess.run(
        ["strings", "-a", str(core)], capture_output=True, text=True, check=True
    ).stdout.splitlines()
    found: set[str] = set()
    for line in strings:
        if EXTENSION_LIST.match(line.strip()):
            found.update(line.strip().split("|"))
    return found


def declared_components() -> dict[str, dict]:
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


def check() -> int:
    """Compare every declared capability with what its binary lists."""
    components = declared_components()
    problems = 0
    for cid, entry in sorted(components.items()):
        binary = entry["artifacts"].get("macos-arm64")
        path = KIT_CORES / binary if binary else None
        if not path or not path.exists():
            continue
        advertised = supported_containers(path)
        declared = set(entry.get("capabilities") or [])
        status = "ok"
        if declared and declared != advertised:
            status = "MISMATCH"
            problems += 1
        elif not declared and advertised:
            status = "undeclared"
        print(f"{cid:<18}{status:<12}advertises {sorted(advertised) or '-'}")
        if status != "ok":
            print(f"{'':<18}{'':<12}declares   {sorted(declared) or '-'}")
    return 1 if problems else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cores", nargs="*", type=Path)
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare every declared capability against its artifact",
    )
    arguments = parser.parse_args()

    if arguments.check:
        return check()
    if not arguments.cores:
        parser.error("give a core path, or --check")
    for core in arguments.cores:
        advertised = advertised_extensions(core)
        print(f"{core.name}")
        print(f"  advertises : {' '.join(sorted(advertised))}")
        print(f"  supported  : {' '.join(sorted(supported_containers(core))) or '(none)'}")
        claimed = advertised & CONTAINER_FORMATS - supported_containers(core)
        if claimed:
            print(f"  ADVERTISED BUT NOT LINKED : {' '.join(sorted(claimed))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
