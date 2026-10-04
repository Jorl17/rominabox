"""Pack a runtime kit and publish it on GitHub, so that a builder on the other
platform can download it.

    uv run python scripts/publish_kit.py pack [KIT] [FOLDER]    # default KIT: desktop/src-tauri/resources/runtime
    uv run python scripts/publish_kit.py upload ARCHIVE...

With `pack`, we write `<platform>-<player>.zip` into FOLDER (default
work/kits) and print its path. KIT is a kit made with scripts/build_kit.py on
its own platform. The player is the first 12 characters of the fork commit
its RetroArch was built from, as recorded in the kit's manifest. The kit's
files are at the root of the archive, each with its Unix permissions, so the
programs of a Mac kit stay executable when someone unpacks it on a Mac.

With `upload`, we publish archives made with `pack` as assets of the release
`kit-<player>` in the repository named in the engine package's Cargo.toml.
We create the release when it does not exist and replace an asset with the
same name. We publish with `gh`, which must be signed in to an account with
write access to the repository. In the builder we form the download URL from
the platform and the player (kits.rs, release_url), and HTTPS is our only
check of a downloaded archive.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KIT = ROOT / "desktop/src-tauri/resources/runtime"
ARCHIVES = ROOT / "work/kits"
ENGINE = ROOT / "desktop/crates/rominabox-engine/Cargo.toml"
NAME = re.compile(r"^(macos|windows)-([0-9a-f]{12})\.zip$")


def repository() -> str:
    """The GitHub repository named in the engine package's Cargo.toml, as owner/name."""
    found = re.search(r'^repository = "https://github\.com/([^"]+)"', ENGINE.read_text(encoding="utf-8"), re.M)
    if not found:
        raise SystemExit(f"{ENGINE} contains no GitHub repository URL")
    return found.group(1)


def identity(manifest: dict) -> tuple[str, str]:
    """A kit's platform and the first 12 characters of its player's commit."""
    player = next(component["revision"] for component in manifest["components"]
                  if component["name"] == "RetroArch")
    return manifest["platform"], player[:12]


def pack(kit: Path, folder: Path) -> Path:
    platform, player = identity(json.loads((kit / "manifest.json").read_text(encoding="utf-8")))
    folder.mkdir(parents=True, exist_ok=True)
    archive = folder / f"{platform}-{player}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as packed:
        for path in sorted(kit.rglob("*")):
            if path.is_symlink():
                raise SystemExit(f"{path} is a link, and a kit may contain only regular files")
            if not path.is_file():
                continue
            info = zipfile.ZipInfo.from_file(path, path.relative_to(kit).as_posix())
            info.compress_type = zipfile.ZIP_DEFLATED
            packed.writestr(info, path.read_bytes())
    return archive


def checked(archive: Path) -> tuple[str, str]:
    """An archive's platform and player, from its name, which must agree with
    the manifest inside it."""
    named = NAME.match(archive.name)
    if not named:
        raise SystemExit(f"{archive.name} is not named <platform>-<player>.zip. Archives are made with `pack`.")
    with zipfile.ZipFile(archive) as packed:
        inside = identity(json.loads(packed.read("manifest.json")))
    if inside != named.groups():
        raise SystemExit(f"{archive.name} contains the {inside[0]} kit of player {inside[1]}")
    return inside


def gh(*arguments: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(["gh", *arguments], capture_output=True, text=True, check=check)


def upload(archives: list[Path]) -> None:
    repo = repository()
    for archive in archives:
        platform, player = checked(archive)
        tag = f"kit-{player}"
        if gh("release", "view", tag, "--repo", repo, check=False).returncode != 0:
            gh("release", "create", tag, "--repo", repo, "--latest=false", "--title", f"Runtime kits for player {player}",
               "--notes", f"The runtime kits of the player built from fork commit {player}, which a builder "
                          "downloads to make a game for the other platform.")
        gh("release", "upload", tag, str(archive), "--repo", repo, "--clobber")
        print(f"{platform} kit of player {player}: https://github.com/{repo}/releases/download/{tag}/{archive.name}")


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == "pack" and len(sys.argv) <= 4:
        kit = Path(sys.argv[2]) if len(sys.argv) > 2 else KIT
        folder = Path(sys.argv[3]) if len(sys.argv) > 3 else ARCHIVES
        print(pack(kit, folder))
        return 0
    if len(sys.argv) >= 3 and sys.argv[1] == "upload":
        upload([Path(name) for name in sys.argv[2:]])
        return 0
    raise SystemExit(__doc__)


if __name__ == "__main__":
    sys.exit(main())
