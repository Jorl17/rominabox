"""Write latest.json, the update manifest of a release, in which every
builder from 0.5.0 on looks for a newer version.

    uv run python scripts/update_manifest.py package <key> <version>
    uv run python scripts/update_manifest.py write <version> <folder> <date>

`package` prints the file name of the package of the platform <key> in
<version>. `write` writes <folder>/latest.json. In <folder> we expect, for
each platform in desktop/src-tauri/update-platforms.json, its package and
the signature of the package, <package>.sig, from `tauri signer sign`. We
refuse to write a manifest with a platform missing, because the updater of
Tauri rejects the whole manifest when one of its entries is incomplete. Each
address names the release of <version>, so a manifest keeps working after a
newer release. <date> is the time of the release in RFC 3339.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import publish_kit  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
PLATFORMS = ROOT / "desktop/src-tauri/update-platforms.json"
MANIFEST = "latest.json"


def platforms() -> list[dict]:
    """The platforms we publish a package for, as declared."""
    declared = json.loads(PLATFORMS.read_text(encoding="utf-8"))
    if declared["formatVersion"] != 1:
        raise SystemExit(f"{PLATFORMS} has the format {declared['formatVersion']}")
    return declared["platforms"]


def package(key: str, version: str) -> str:
    """The file name of the package of the platform `key` in `version`."""
    for platform in platforms():
        if platform["key"] == key:
            return platform["package"].format(version=version)
    raise SystemExit(f"{PLATFORMS} declares no platform {key}")


def manifest(version: str, folder: Path, date: str, repository: str) -> dict:
    """The manifest of `version`, whose packages and signatures are in `folder`."""
    entries, missing = {}, []
    for platform in platforms():
        name = platform["package"].format(version=version)
        signature = folder / f"{name}.sig"
        if not (folder / name).is_file() or not signature.is_file():
            missing.append(name if not (folder / name).is_file() else signature.name)
            continue
        entries[platform["key"]] = {
            "signature": signature.read_text(encoding="utf-8").strip(),
            "url": f"https://github.com/{repository}/releases/download/v{version}/{name}",
        }
    if missing:
        raise SystemExit(f"no update manifest: {folder} lacks {', '.join(missing)}")
    return {"version": version, "notes": f"ROM-in-a-Box {version}", "pub_date": date, "platforms": entries}


def main() -> int:
    arguments = sys.argv[1:]
    if len(arguments) == 3 and arguments[0] == "package":
        print(package(arguments[1], arguments[2]))
        return 0
    if len(arguments) == 4 and arguments[0] == "write":
        version, folder, date = arguments[1], Path(arguments[2]), arguments[3]
        written = manifest(version, folder, date, publish_kit.repository())
        (folder / MANIFEST).write_text(json.dumps(written, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {folder / MANIFEST}")
        return 0
    raise SystemExit(__doc__)


if __name__ == "__main__":
    raise SystemExit(main())
