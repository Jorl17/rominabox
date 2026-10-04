"""Check that a packed kit has the name that we look for in the builder, and
that we refuse an archive whose name and contents disagree before any
upload.

    uv run python scripts/test_publish_kit.py

We pack a made-up kit in a temporary folder, upload nothing and do not run
`gh`.
"""

from __future__ import annotations

import json
import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import publish_kit  # noqa: E402
import scratch  # noqa: E402

PLAYER = "1dc041423fc677352ae041a9ae1439fa7203f7d4"


def made_kit(folder: Path, platform: str, player: str) -> Path:
    (folder / "bin").mkdir(parents=True)
    (folder / "manifest.json").write_text(json.dumps(
        {"platform": platform, "components": [{"name": "RetroArch", "revision": player}]}), encoding="utf-8")
    (folder / "bin/retroarch").write_bytes(b"player")
    return folder


def main() -> int:
    failures = []
    with scratch.scratch("rominabox-publish-kit-") as temporary:
        root = Path(temporary)
        archive = publish_kit.pack(made_kit(root / "kit", "windows", PLAYER), root / "out")
        if archive.name != "windows-1dc041423fc6.zip":
            failures.append(f"packed as {archive.name}")
        with zipfile.ZipFile(archive) as packed:
            if sorted(packed.namelist()) != ["bin/retroarch", "manifest.json"]:
                failures.append(f"holds {packed.namelist()}")
        if publish_kit.checked(archive) != ("windows", "1dc041423fc6"):
            failures.append("a packed archive is not accepted for upload")
        renamed = archive.rename(root / "out" / "macos-1dc041423fc6.zip")
        try:
            publish_kit.checked(renamed)
            failures.append("an archive named for the Mac holding the Windows kit was accepted")
        except SystemExit as refusal:
            if "holds the windows kit" not in str(refusal):
                failures.append(f"refused for another reason: {refusal}")
        if publish_kit.repository() != "Jorl17/rominabox":
            failures.append(f"publishes to {publish_kit.repository()}")
    for failure in failures:
        print(f"  FAIL {failure}")
    print("publish_kit: every case holds" if not failures else f"{len(failures)} case(s) failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
