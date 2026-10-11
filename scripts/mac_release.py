"""The last steps of a Mac builder in the release workflow, the same for the
arm64, Intel and universal builders.

    uv run python scripts/mac_release.py check <app> <processor>...
    uv run python scripts/mac_release.py pack <app> <name> <key> <version> <folder>

`check` refuses a builder whose program, command line or menu preview
renderer has code for other processors than <processor>... `pack` writes
ROM-in-a-Box-<version>-<name>.zip into <folder>. When NOTARIZE is "yes", we
first notarize the app with the App Store Connect key in NOTARY_KEY,
NOTARY_KEY_ID and NOTARY_ISSUER, staple the ticket, and also write the update
package of the platform <key> (scripts/update_manifest.py), without the
extended attributes of macOS. We offer no update package of a builder that
is not notarized.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_builder import processors_of  # noqa: E402
import update_manifest  # noqa: E402


def check(app: Path, processors: list[str]) -> None:
    programs = [*sorted((app / "Contents/MacOS").iterdir()), app / "Contents/Resources/bin/rominabox-cli",
                app / "Contents/Resources/preview/rml-preview"]
    for program in programs:
        if processors_of(program) != sorted(processors):
            raise SystemExit(f"{program} holds {processors_of(program)}, not {sorted(processors)}")


def notarize(app: Path) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        key = Path(temporary) / "notary.p8"
        key.write_text(os.environ["NOTARY_KEY"], encoding="utf-8")
        archive = Path(temporary) / "notarize.zip"
        subprocess.run(["ditto", "-c", "-k", "--keepParent", str(app), str(archive)], check=True)
        subprocess.run(["xcrun", "notarytool", "submit", str(archive), "--key", str(key),
                        "--key-id", os.environ["NOTARY_KEY_ID"], "--issuer", os.environ["NOTARY_ISSUER"],
                        "--wait"], check=True)
    subprocess.run(["xcrun", "stapler", "staple", str(app)], check=True)


def pack(app: Path, name: str, key: str, version: str, folder: Path) -> None:
    folder.mkdir(parents=True, exist_ok=True)
    if os.environ.get("NOTARIZE") == "yes":
        notarize(app)
        package = folder / update_manifest.package(key, version)
        subprocess.run(["tar", "--no-mac-metadata", "-czf", str(package), "-C", str(app.parent), app.name],
                       env={**os.environ, "COPYFILE_DISABLE": "1"}, check=True)
    subprocess.run(["ditto", "-c", "-k", "--keepParent", str(app),
                    str(folder / f"ROM-in-a-Box-{version}-{name}.zip")], check=True)


def main() -> int:
    arguments = sys.argv[1:]
    if len(arguments) >= 3 and arguments[0] == "check":
        check(Path(arguments[1]), arguments[2:])
        return 0
    if len(arguments) == 6 and arguments[0] == "pack":
        pack(Path(arguments[1]), arguments[2], arguments[3], arguments[4], Path(arguments[5]))
        return 0
    raise SystemExit(__doc__)


if __name__ == "__main__":
    raise SystemExit(main())
