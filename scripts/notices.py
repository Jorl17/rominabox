"""Collect installed runtime notices and record exact dependency versions for local bundles."""

from __future__ import annotations

import importlib.metadata
import json
import shutil
import sys
import urllib.request
from pathlib import Path

DEPENDENCIES = (
    "PySide6-Essentials",
    "shiboken6",
    "Pillow",
    "pydantic",
    "pydantic_core",
    "annotated-types",
    "typing-extensions",
    "typing-inspection",
    "PyInstaller",
)


def collect(destination: Path, cache: Path, *, dependencies: tuple[str, ...] = DEPENDENCIES) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    components = []
    for name in dependencies:
        dist = importlib.metadata.distribution(name)
        directory = destination / name
        directory.mkdir(exist_ok=True)
        for file in dist.files or ():
            if file.name.lower().startswith(("license", "copying")) and file.suffix not in {".py", ".pyc"}:
                shutil.copy2(dist.locate_file(file), directory / file.name)
        components.append(
            {"name": name, "version": dist.version, "project_urls": dist.metadata.get_all("Project-URL") or []}
        )
    qt = destination / "Qt-and-PySide"
    qt.mkdir(exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    for name in ("LGPL-3.0-only.txt", "GPL-3.0-only.txt"):
        path = cache / name
        if not path.exists():
            url = f"https://raw.githubusercontent.com/pyside/pyside-setup/v6.9.3/LICENSES/{name}"
            with urllib.request.urlopen(url, timeout=20) as response:
                path.write_bytes(response.read())
        shutil.copy2(path, qt / name)
    python_license = (
        Path(sys.base_prefix) / "lib" / f"python{sys.version_info.major}.{sys.version_info.minor}" / "LICENSE.txt"
    )
    if python_license.exists():
        shutil.copy2(python_license, destination / "Python-LICENSE.txt")
    components.append(
        {"name": "Python", "version": sys.version.split()[0], "source": "https://www.python.org/downloads/source/"}
    )
    (destination / "components.json").write_text(json.dumps(components, indent=2))
    (destination / "README.txt").write_text(
        "These notices cover identified application dependencies, not ROM-in-a-Box's own license.\n"
        "Qt/PySide 6.9.3 sources: https://download.qt.io/official_releases/QtForPython/pyside6/PySide6-6.9.3-src/\n"
        "Qt 6.9.3 sources: https://download.qt.io/official_releases/qt/6.9/6.9.3/single/\n"
        "This local prototype still needs a complete library and corresponding-source audit before public release.\n"
    )
