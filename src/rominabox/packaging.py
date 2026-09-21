from __future__ import annotations

import json
import plistlib
import shutil
import subprocess
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

from PIL import Image, ImageOps

from .domain import SYSTEMS, AppError, Project, file_hash
from .templates import default_template, validate_template

Progress = Callable[[str], None]


def available_systems(kit: Path) -> list[str]:
    """Report systems with a core present, separately from configured console definitions."""
    return [s.id for s in SYSTEMS.values() if any((kit / "cores").glob(f"{s.core}_libretro.*"))]


def validate(project: Project, kit: Path) -> dict[str, Any]:
    """Check inputs and pinned runtime artifacts before allocating a game package."""
    if not project.rom.is_file():
        raise AppError("rom_missing", "The game file could not be found.")
    if project.rom.suffix.lower() in {".cue", ".gdi", ".m3u"}:
        raise AppError("multifile_pending", "Multi-file disc packaging is not available in this prototype.")
    if project.system not in available_systems(kit):
        raise AppError("core_missing", f"{SYSTEMS[project.system].name} needs a runtime core that is not in this kit.")
    try:
        manifest = json.loads((kit / "components.json").read_text())
        if manifest["platform"] != "darwin":
            raise AppError("target_pending", "This prototype currently exports macOS applications.")
        selected = SYSTEMS[project.system].core
        required = {selected, "RetroArch"}
        entries = {entry["name"]: entry for entry in manifest["components"]}
        for name in required:
            entry = entries[name]
            binary = kit / (
                "RetroArch.app/Contents/MacOS/RetroArch" if name == "RetroArch" else f"cores/{name}_libretro.dylib"
            )
            if file_hash(binary) != entry["binary_sha256"]:
                raise AppError("runtime_changed", f"{name} does not match the runtime manifest. Prepare the kit again.")
            source_name = entry["source_archive"]
            if not isinstance(source_name, str) or Path(source_name).name != source_name:
                raise AppError("invalid_source", "Invalid source archive name in the runtime manifest.")
            source = kit / "sources" / source_name
            if not source.is_file() or file_hash(source) != entry["source_sha256"]:
                raise AppError("source_missing", f"The matching source archive for {name} is missing or changed.")
            if not (kit / "licenses" / f"{name}.txt").is_file():
                raise AppError("notice_missing", f"The runtime kit is missing the license for {name}.")
        for asset in (project.icon, project.background):
            if asset is not None:
                with Image.open(asset) as picture:
                    picture.verify()
        return dict(manifest)
    except (OSError, ValueError, KeyError, TypeError) as exc:
        raise AppError("invalid_kit", f"An input or runtime file is invalid: {exc}") from exc


def sign_app(path: Path) -> None:
    """Apply a local ad-hoc signature. Public distribution requires a release signing workflow."""
    result = subprocess.run(["codesign", "--force", "--sign", "-", str(path)], capture_output=True, text=True)
    if result.returncode:
        raise AppError("sign_failed", f"Could not sign the application: {result.stderr.strip()}")


def bundle_runtime(project: Project, kit: Path, destination: Path, manifest: dict[str, Any]) -> None:
    """Stage the selected core and its required runtime and provenance from an explicit allowlist."""
    if destination.is_symlink():
        destination.unlink()
    elif destination.exists():
        shutil.rmtree(destination)
    destination.mkdir()
    selected = SYSTEMS[project.system].core
    components = [entry for entry in manifest["components"] if entry["name"] in {selected, "RetroArch"}]
    shutil.copytree(kit / "RetroArch.app", destination / "RetroArch.app", symlinks=True)
    files = [Path("cores") / f"{selected}_libretro.dylib"]
    info = Path("info") / f"{selected}_libretro.info"
    if (kit / info).is_file():
        files.append(info)
    for component in components:
        files.extend((Path("licenses") / f"{component['name']}.txt", Path("sources") / component["source_archive"]))
    if (kit / "licenses/README.txt").is_file():
        files.append(Path("licenses/README.txt"))
    for relative in files:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(kit / relative, target)
    selected_manifest = {key: value for key, value in manifest.items() if key != "catalog_revision"}
    selected_manifest["components"] = components
    (destination / "components.json").write_text(json.dumps(selected_manifest, indent=2))


def build(
    project: Project,
    output: Path,
    kit: Path,
    template: Path | None = None,
    progress: Progress = lambda _: None,
    signer: Callable[[Path], None] = sign_app,
) -> Path:
    """Assemble a standalone Mac game atomically. A callback may raise to cancel before publication."""
    progress("Checking game and runtime…")
    manifest = validate(project, kit)
    template = template or default_template(project.theme)
    validate_template(template, project.theme)
    output = output.expanduser().resolve()
    output.mkdir(parents=True, exist_ok=True)
    destination = output / f"{project.title}.app"
    if destination.exists():
        raise AppError("output_exists", "An app with that name already exists. Choose another name or folder.")
    with tempfile.TemporaryDirectory(prefix=".rominabox-", dir=output) as temporary:
        staged = Path(temporary) / destination.name
        progress("Copying application…")
        shutil.copytree(template, staged, symlinks=True)
        resources = staged / "Contents/Resources"
        resources.mkdir(exist_ok=True)
        target_kit = resources / "runtime-kit"
        progress("Bundling emulator…")
        bundle_runtime(project, kit, target_kit, manifest)
        content = resources / "game"
        if content.exists():
            shutil.rmtree(content)
        content.mkdir()
        packaged = project.model_copy(deep=True)
        rom_name = "game" + project.rom.suffix.lower()
        shutil.copy2(project.rom, content / rom_name)
        packaged.rom = Path("game") / rom_name
        for name in ("icon", "background"):
            asset = getattr(project, name)
            if asset is not None:
                with Image.open(asset) as picture:
                    picture.convert("RGBA").save(content / f"{name}.png")
                setattr(packaged, name, Path("game") / f"{name}.png")
        if project.icon:
            with Image.open(project.icon) as picture:
                icon = ImageOps.pad(picture.convert("RGBA"), (1024, 1024), color=(0, 0, 0, 0))
                icon.save(resources / "game.icns", format="ICNS")
        packaged.write(resources / "player.json")
        plist_path = staged / "Contents/Info.plist"
        info = plistlib.loads(plist_path.read_bytes())
        info.update(
            CFBundleName=project.title,
            CFBundleDisplayName=project.title,
            CFBundleIdentifier=f"org.rominabox.game.{project.game_id}",
            NSHumanReadableCopyright=project.description,
        )
        if project.icon:
            info["CFBundleIconFile"] = "game.icns"
        plist_path.write_bytes(plistlib.dumps(info))
        progress("Signing application…")
        signer(staged)
        progress("Finishing…")
        staged.rename(destination)
    return destination
