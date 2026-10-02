"""Pack a runtime kit into the archive that a builder on the other platform
downloads, and print the entry for it in `desktop/kits.json`.

    uv run python scripts/pack_kit.py KIT ARCHIVE.zip

KIT is a kit made with scripts/build_kit.py (desktop/src-tauri/resources/runtime
on the machine that built it). The archive has the files of the kit at its
root, each with its Unix permissions, so the programs of a Mac kit unpacked
on a Mac stay executable. The printed entry contains the platform and player
of the kit (the fork commit its RetroArch was built from) and the SHA-256 of
the archive. We fill in its url, where the archive is published, by hand.
"""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import Path


def pack(kit: Path, archive: Path) -> dict:
    manifest = json.loads((kit / "manifest.json").read_text(encoding="utf-8"))
    player = next(component["revision"] for component in manifest["components"]
                  if component["name"] == "RetroArch")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as packed:
        for path in sorted(kit.rglob("*")):
            if path.is_symlink():
                raise SystemExit(f"{path} is a link; a kit holds files only")
            if not path.is_file():
                continue
            info = zipfile.ZipInfo.from_file(path, path.relative_to(kit).as_posix())
            info.compress_type = zipfile.ZIP_DEFLATED
            packed.writestr(info, path.read_bytes())
    return {
        "platform": manifest["platform"],
        "player": player,
        "url": "",
        "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
    }


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    entry = pack(Path(sys.argv[1]), Path(sys.argv[2]))
    print(json.dumps(entry, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
