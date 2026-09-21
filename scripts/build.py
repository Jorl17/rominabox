"""Freeze the authoring app and one player-only template per declared theme."""

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

from make_icon import make_icon
from notices import DEPENDENCIES, collect
from PyInstaller.archive.readers import CArchiveReader

from rominabox.themes import THEMES, Theme

ROOT = Path(__file__).resolve().parents[1]
AUTHORING_MODULES = (
    "rominabox.cli",
    "rominabox.catalog",
    "rominabox.packaging",
    "rominabox.templates",
    "rominabox.ui.builder",
    "rominabox.ui.common",
    "rominabox.ui.preview",
)


def frozen_modules(executable: Path) -> set[str]:
    """Inspect the executable's actual Python archive, rather than trusting freeze arguments."""
    archive = CArchiveReader(str(executable))
    pyz_name = next(name for name in archive.toc if name.endswith(".pyz"))
    return set(archive.open_embedded_archive(pyz_name).toc)


def freeze(theme: Theme | None, icon: Path) -> tuple[Path, set[str]]:
    """Build a standalone app; a selected theme restricts this build to the player and its renderer."""
    role = f"player-{theme.id}" if theme else "builder"
    distribution = ROOT / "work/frozen" / role if theme else ROOT / "dist"
    selected = [theme] if theme else list(THEMES.values())
    excluded = list(AUTHORING_MODULES) if theme else []
    if theme:
        excluded += [other.module for other in THEMES.values() if other.module != theme.module]
    command = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        "--windowed",
        "--name",
        "ROM-in-a-Box",
        "--osx-bundle-identifier",
        f"org.rominabox.{role}",
        "--paths",
        str(ROOT / "src"),
        "--distpath",
        str(distribution),
        "--workpath",
        str(ROOT / "build" / role),
        "--specpath",
        str(ROOT / "work/specs" / role),
    ]
    if sys.platform == "darwin":
        command += ["--icon", str(icon)]
    for name in excluded:
        command += ["--exclude-module", name]
    assets = set()
    for item in selected:
        command += ["--hidden-import", item.module]
        assets.update(item.assets)
    for asset in sorted(assets):
        relative = Path(asset)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"Theme asset must be package-relative: {asset}")
        source = ROOT / "src/rominabox" / relative
        if not source.exists():
            raise FileNotFoundError(source)
        command += ["--add-data", f"{source}:{Path('rominabox') / relative.parent}"]
    command += [str(ROOT / "scripts" / ("player_entry.py" if theme else "entry.py"))]
    environment = dict(os.environ, PYINSTALLER_CONFIG_DIR=str(ROOT / "work/pyinstaller-cache"))
    subprocess.run(command, cwd=ROOT, env=environment, check=True)
    app = distribution / ("ROM-in-a-Box.app" if sys.platform == "darwin" else "ROM-in-a-Box")
    executable = app / (
        "Contents/MacOS/ROM-in-a-Box"
        if sys.platform == "darwin"
        else "ROM-in-a-Box" + (".exe" if sys.platform == "win32" else "")
    )
    modules = frozen_modules(executable)
    if theme:
        assert theme.module in modules, f"Missing selected theme: {theme.id}"
        unwanted = {name for name in modules if any(name == item or name.startswith(item + ".") for item in excluded)}
        assert not unwanted, f"Unexpected player modules: {unwanted}"
        assert "rominabox.ui.player" in modules
    return app, modules


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-kit", type=Path, default=ROOT / "work/runtime-kit")
    parser.add_argument("--without-runtime", action="store_true", help="Build application shells and templates for CI")
    args = parser.parse_args()
    if not args.without_runtime:
        if sys.platform != "darwin":
            parser.error("Runtime assembly currently requires macOS.")
        if not (args.runtime_kit / "components.json").exists():
            parser.error("Prepare the runtime kit first with scripts/prepare_runtime.py.")
    icon = ROOT / "work/app.icns"
    icon.parent.mkdir(exist_ok=True)
    if sys.platform == "darwin":
        make_icon(icon)
    templates = ROOT / "dist/player-templates"
    if templates.exists():
        shutil.rmtree(templates)
    templates.mkdir(parents=True)
    for theme in THEMES.values():
        app, modules = freeze(theme, icon)
        resources = app / "Contents/Resources" if sys.platform == "darwin" else app / "_internal"
        resources.mkdir(parents=True, exist_ok=True)
        (resources / "player-template.json").write_text(
            json.dumps({"schema_version": 1, "kind": "player", "theme": theme.id}, indent=2)
        )
        if not args.without_runtime:
            dependencies = tuple(name for name in DEPENDENCIES if name != "Pillow" or "PIL" in modules)
            collect(resources / "dependency-notices", ROOT / "work/license-cache", dependencies=dependencies)
        if sys.platform == "darwin":
            subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)
        destination = templates / (f"{theme.id}.app" if sys.platform == "darwin" else theme.id)
        shutil.copytree(app, destination, symlinks=True)
        print(f"Verified player template: {theme.id}; authoring and unselected theme modules absent.", flush=True)
    app, _ = freeze(None, icon)
    resources = app / "Contents/Resources" if sys.platform == "darwin" else app / "_internal"
    shutil.copytree(templates, resources / "player-templates", symlinks=True)
    if not args.without_runtime:
        shutil.copytree(args.runtime_kit, resources / "runtime-kit", symlinks=True)
        shutil.rmtree(resources / "runtime-kit/headless", ignore_errors=True)
        collect(resources / "dependency-notices", ROOT / "work/license-cache")
    if sys.platform == "darwin":
        subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)


if __name__ == "__main__":
    main()
