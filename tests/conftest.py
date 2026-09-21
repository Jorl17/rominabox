import json
import plistlib

import pytest

from rominabox.domain import file_hash


@pytest.fixture
def package_inputs(tmp_path):
    kit = tmp_path / "kit"
    entries = []
    for name, relative in [
        ("gambatte", "cores/gambatte_libretro.dylib"),
        ("RetroArch", "RetroArch.app/Contents/MacOS/RetroArch"),
    ]:
        path = kit / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(f"fake external binary: {name}".encode())
        license_path = kit / "licenses" / f"{name}.txt"
        license_path.parent.mkdir(exist_ok=True)
        license_path.write_text(f"License fixture for {name}")
        source = kit / "sources" / f"{name}.tar.gz"
        source.parent.mkdir(exist_ok=True)
        source.write_bytes(b"source fixture")
        entries.append(
            {
                "name": name,
                "binary_sha256": file_hash(path),
                "source_archive": source.name,
                "source_sha256": file_hash(source),
            }
        )
    (kit / "components.json").write_text(json.dumps({"platform": "darwin", "components": entries}))
    template = tmp_path / "Player.app"
    executable = template / "Contents/MacOS/ROM-in-a-Box"
    executable.parent.mkdir(parents=True)
    executable.write_bytes(b"fake frozen application")
    (template / "Contents/Info.plist").write_bytes(plistlib.dumps({"CFBundleExecutable": "ROM-in-a-Box"}))
    marker = template / "Contents/Resources/player-template.json"
    marker.parent.mkdir(parents=True)
    marker.write_text(json.dumps({"schema_version": 1, "kind": "player", "theme": "8-bit"}))
    return kit, template
