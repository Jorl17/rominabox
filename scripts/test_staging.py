"""Check that the paths in the staging script exist.

In `scripts/native_runtime/build-builder-macos.sh` we assemble the runtime kit
from directories written as literals. A rename that also matched a similar
name could point the script at a directory that does not exist, for example
`desktop/assets/menu-sounds` when `desktop/assets/menu` moves into a design
package. The script would then fail on the next build, far from the change
responsible.

    python3 scripts/test_staging.py

We do not run the script here, because it builds a whole application. We
read the paths in it and check that they exist.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/native_runtime/build-builder-macos.sh"

# Assignments of the form name="$root/some/path" or name="$other_var/sub",
# because in the script we build some paths from earlier ones, menu_source
# among them. With only the $root form we would miss those.
ASSIGNMENT = re.compile(r'^\s*(\w+)="\$(\w+)/([^"$]+)"\s*$', re.MULTILINE)

# We create the destinations in the build, so only sources must already exist.
DESTINATIONS = {"shared_assets", "sound_staging", "app"}


def builder_uses_cargo_target(text: str) -> bool:
    return "from built import target_dir" in text and all(
        path in text
        for path in (
            'staging="$cargo_target_dir/$profile"',
            'cp "$cargo_target_dir/release/rominabox-cli" resources/bin/rominabox-cli',
            'app="$cargo_target_dir/release/bundle/macos/ROM-in-a-Box.app"',
        )
    )


def main() -> int:
    if not SCRIPT.exists():
        raise SystemExit(f"no staging script at {SCRIPT}")
    text = SCRIPT.read_text()
    failures: list[str] = []
    checked = 0

    # Cargo may write into the shared target outside this checkout. For the
    # builder we must use the same target directory as in the other build
    # scripts, for the permission pass, the CLI copy and the final app bundle.
    if not builder_uses_cargo_target(text):
        failures.append("cargo_target_dir")
        print("  FAIL builder does not use built.target_dir for every Cargo output")

    # Resolve one level of indirection: a name defined earlier can be the base
    # of a later path.
    resolved: dict[str, Path] = {"root": ROOT}
    for name, base, relative in ASSIGNMENT.findall(text):
        if base not in resolved:
            print(f"  skip {name:<18}built from ${base}, which this check cannot resolve")
            continue
        target = resolved[base] / relative
        resolved[name] = target
        if name in DESTINATIONS:
            continue
        checked += 1
        if target.exists():
            print(f"  ok   {name:<18}{relative}")
        else:
            print(f"  FAIL {name:<18}{relative} does not exist")
            failures.append(name)

    if not checked:
        raise SystemExit(
            "no $root paths were found in the staging script; this check has "
            "stopped matching how the script is written and is proving nothing"
        )

    stale = staged_designs_are_current()
    for entry in stale:
        print(f"  STALE {entry}")

    if failures:
        print(f"\n{len(failures)} staging path(s) point at nothing: {', '.join(failures)}")
        return 1
    if stale:
        print(
            f"\n{len(stale)} staged design file(s) are older than the design they "
            "came from, so the builder and an exported game draw something the "
            "tests never render. Restage:\n"
            "  sh scripts/native_runtime/build-builder-macos.sh",
        )
        return 1
    print(f"\nall {checked} staging source paths exist, and the kit's designs are current")
    return 0



DESIGNS = ROOT / "integrations/designs"
KIT_DESIGNS = ROOT / "desktop/src-tauri/resources/runtime/designs"


def staged_designs_are_current() -> list[str]:
    """Check that the kit's copy of each design matches the design we staged.

    The kit is build output and not a second copy that we maintain by hand, so
    this checks a cache for staleness. A kit staged before a design changed
    can contain an older document, for example an older frame, while every
    test stages from the source.
    """
    stale: list[str] = []
    for design in sorted(p for p in DESIGNS.iterdir() if p.is_dir()):
        staged = KIT_DESIGNS / design.name
        if not staged.is_dir():
            stale.append(f"{design.name}: not in the kit at all")
            continue
        for document in sorted(p for p in design.iterdir() if p.is_file()):
            beside = staged / document.name
            if not beside.exists():
                stale.append(f"{design.name}/{document.name}: missing from the kit")
            elif beside.read_bytes() != document.read_bytes():
                stale.append(f"{design.name}/{document.name}: the kit's copy is older")
    return stale


if __name__ == "__main__":
    sys.exit(main())
