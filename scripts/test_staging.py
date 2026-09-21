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
DESTINATIONS = {"menu_assets", "sound_staging", "app"}


def main() -> int:
    if not SCRIPT.exists():
        raise SystemExit(f"no staging script at {SCRIPT}")
    text = SCRIPT.read_text()
    failures: list[str] = []
    checked = 0

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

    if failures:
        print(f"\n{len(failures)} staging path(s) point at nothing: {', '.join(failures)}")
        return 1
    print(f"\nall {checked} staging source paths exist")
    return 0


if __name__ == "__main__":
    sys.exit(main())
