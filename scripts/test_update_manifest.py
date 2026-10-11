"""Check the update manifest of a release: an entry for every declared
platform, each with the signature of its package and the address of the
package in the release of its version, and no manifest at all when a
package or a signature is missing.

    uv run python scripts/test_update_manifest.py

We write made-up packages and signatures in a temporary folder, and upload
nothing.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import scratch  # noqa: E402
import update_manifest  # noqa: E402

VERSION = "0.5.1"
DATE = "2026-10-11T12:00:00Z"
REPOSITORY = "Jorl17/rominabox"


def released(folder: Path) -> dict[str, str]:
    """A package and a signature in `folder` for every declared platform, and
    the signature of each package by its key."""
    folder.mkdir(parents=True)
    signatures = {}
    for platform in update_manifest.platforms():
        name = platform["package"].format(version=VERSION)
        (folder / name).write_bytes(b"package")
        signatures[platform["key"]] = f"signature of {name}"
        (folder / f"{name}.sig").write_text(signatures[platform["key"]] + "\n", encoding="utf-8")
    return signatures


def main() -> int:
    failures = []
    keys = {platform["key"] for platform in update_manifest.platforms()}
    if keys != {"darwin-aarch64", "darwin-x86_64", "darwin-universal", "windows-x86_64"}:
        failures.append(f"declared platforms {sorted(keys)}")
    with scratch.scratch("rominabox-update-manifest-") as temporary:
        root = Path(temporary)

        complete = root / "complete"
        signatures = released(complete)
        written = update_manifest.manifest(VERSION, complete, DATE, REPOSITORY)
        if (written["version"], written["pub_date"]) != (VERSION, DATE):
            failures.append(f"version and date {written['version']}, {written['pub_date']}")
        if set(written["platforms"]) != keys:
            failures.append(f"entries {sorted(written['platforms'])}")
        for key, entry in written["platforms"].items():
            name = update_manifest.package(key, VERSION)
            if entry["signature"] != signatures[key]:
                failures.append(f"{key}: signature {entry['signature']!r}")
            if entry["url"] != f"https://github.com/{REPOSITORY}/releases/download/v{VERSION}/{name}":
                failures.append(f"{key}: address {entry['url']}")

        for missing in ("ROM-in-a-Box-0.5.1-macos-universal.app.tar.gz.sig", "ROM-in-a-Box_0.5.1_x64-setup.exe"):
            partial = root / f"without-{missing}"
            released(partial)
            (partial / missing).unlink()
            try:
                update_manifest.manifest(VERSION, partial, DATE, REPOSITORY)
                failures.append(f"a manifest without {missing}")
            except SystemExit as refusal:
                if missing not in str(refusal):
                    failures.append(f"without {missing}, the refusal says {refusal}")

        # The command line writes the manifest into the folder of the packages.
        command = subprocess.run(
            [sys.executable, str(Path(update_manifest.__file__)), "write", VERSION, str(complete), DATE],
            capture_output=True, text=True)
        if command.returncode != 0:
            failures.append(f"write failed: {command.stderr}")
        elif json.loads((complete / update_manifest.MANIFEST).read_text(encoding="utf-8"))["version"] != VERSION:
            failures.append("the written manifest has another version")
        named = subprocess.run(
            [sys.executable, str(Path(update_manifest.__file__)), "package", "darwin-universal", VERSION],
            capture_output=True, text=True)
        if named.stdout.strip() != "ROM-in-a-Box-0.5.1-macos-universal.app.tar.gz":
            failures.append(f"package name {named.stdout.strip()!r} {named.stderr}")

    for failure in failures:
        print(f"FAIL {failure}")
    print("update manifest: " + ("ok" if not failures else f"{len(failures)} failure(s)"))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
