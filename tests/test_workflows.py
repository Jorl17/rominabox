import json
import plistlib
import subprocess
import sys

import pytest
from PIL import Image
from PySide6.QtCore import Qt

from rominabox import packaging
from rominabox.cli import main
from rominabox.domain import AppError, Project
from rominabox.ui.builder import Builder


def create_project(tmp_path):
    rom = tmp_path / "my game.gbc"
    rom.write_bytes(bytes(512))
    return Project.from_rom(rom)


def test_cli_inspect_build_and_reopen_project(tmp_path, package_inputs, monkeypatch, capsys):
    kit, template = package_inputs
    project = create_project(tmp_path)
    assert main(["inspect", str(project.rom)]) == 0
    inspected = Project.model_validate_json(capsys.readouterr().out)
    image = tmp_path / "art.png"
    Image.new("RGB", (80, 120), "#40aa90").save(image)
    inspected.icon = image
    inspected.title = "A game é"
    config = tmp_path / "project.json"
    inspected.write(config)
    # We replace only the platform signer. Assembly, input validation and CLI are unchanged.
    monkeypatch.setattr(subprocess, "run", lambda *args, **kwargs: subprocess.CompletedProcess(args, 0, "", ""))
    assert (
        main(
            [
                "build",
                str(config),
                "--runtime-kit",
                str(kit),
                "--template",
                str(template),
                "--output",
                str(tmp_path / "out"),
            ]
        )
        == 0
    )
    path = tmp_path / "out/A game é.app"
    response = json.loads(capsys.readouterr().out)
    assert response["path"] == str(path)
    exported = Project.read(path / "Contents/Resources/player.json")
    assert exported.rom.read_bytes() == project.rom.read_bytes()
    assert exported.game_id == project.game_id
    assert exported.icon.is_file()
    assert (path / "Contents/Resources/game.icns").is_file()
    assert plistlib.loads((path / "Contents/Info.plist").read_bytes())["CFBundleName"] == "A game é"
    assert str(tmp_path) not in (path / "Contents/Resources/player.json").read_text()
    assert (
        main(
            [
                "build",
                str(config),
                "--runtime-kit",
                str(kit),
                "--template",
                str(template),
                "--output",
                str(tmp_path / "out"),
            ]
        )
        == 1
    )
    assert json.loads(capsys.readouterr().err)["error"]["code"] == "output_exists"


def test_cli_process_has_json_errors(tmp_path):
    result = subprocess.run(
        [sys.executable, "-m", "rominabox.cli", "inspect", str(tmp_path / "missing.gbc")],
        text=True,
        capture_output=True,
    )
    assert result.returncode == 1
    assert json.loads(result.stderr)["error"]["code"] == "rom_missing"


def test_build_failure_and_cancel_never_publish_partial_app(tmp_path, package_inputs):
    kit, template = package_inputs
    project = create_project(tmp_path)

    def fail_signing(path):
        raise AppError("sign_failed", "Signing failed")

    output = tmp_path / "out"
    with pytest.raises(AppError, match="Signing failed"):
        packaging.build(project, output, kit, template, signer=fail_signing)
    assert list(output.iterdir()) == []

    def cancel(message):
        if message == "Bundling emulator…":
            raise AppError("cancelled", "Build cancelled")

    with pytest.raises(AppError, match="cancelled"):
        packaging.build(project, output, kit, template, progress=cancel, signer=lambda _: None)
    assert list(output.iterdir()) == []


def test_changed_runtime_rejected_before_copy(tmp_path, package_inputs):
    kit, template = package_inputs
    project = create_project(tmp_path)
    (kit / "cores/gambatte_libretro.dylib").write_bytes(b"changed")
    with pytest.raises(AppError, match="does not match"):
        packaging.build(project, tmp_path / "out", kit, template, signer=lambda _: None)
    assert not (tmp_path / "out").exists()


def test_builder_import_options_build_workflow(qtbot, tmp_path, package_inputs, monkeypatch):
    kit, template = package_inputs
    project = create_project(tmp_path)
    monkeypatch.setattr(subprocess, "run", lambda *args, **kwargs: subprocess.CompletedProcess(args, 0, "", ""))
    widget = Builder(kit, template)
    qtbot.addWidget(widget)
    widget.lookup.setChecked(False)
    widget.show()
    widget.select_rom(project.rom)
    qtbot.waitUntil(lambda: widget.project is not None)
    widget.title.setText("GUI game")
    qtbot.mouseClick(widget.next, Qt.MouseButton.LeftButton)
    widget.fullscreen.setChecked(True)
    widget.accent.setCurrentIndex(2)
    qtbot.mouseClick(widget.next, Qt.MouseButton.LeftButton)
    assert widget.pages.currentIndex() == 2
    widget.start_build(tmp_path / "out")
    qtbot.waitUntil(lambda: widget.output is not None)
    qtbot.waitUntil(lambda: not widget.job.isRunning())
    exported = Project.read(widget.output / "Contents/Resources/player.json")
    assert exported.title == "GUI game"
    assert exported.fullscreen is True
    assert exported.accent == "#00683d"
    assert exported.game_id == project.game_id


def test_dropping_second_console_redetects_system(qtbot, tmp_path, package_inputs):
    kit, template = package_inputs
    gameboy = create_project(tmp_path)
    sega = tmp_path / "game.md"
    sega.write_bytes(bytes(512))
    widget = Builder(kit, template)
    qtbot.addWidget(widget)
    widget.lookup.setChecked(False)
    widget.select_rom(gameboy.rom)
    qtbot.waitUntil(lambda: widget.project is not None and not widget.job.isRunning())
    widget.select_rom(sega)
    qtbot.waitUntil(lambda: widget.project.rom == sega and not widget.job.isRunning())
    assert widget.project.system == "megadrive"


def test_missing_corresponding_source_prevents_export(tmp_path, package_inputs):
    kit, template = package_inputs
    project = create_project(tmp_path)
    (kit / "sources/gambatte.tar.gz").unlink()
    with pytest.raises(AppError, match="source"):
        packaging.build(project, tmp_path / "out", kit, template, signer=lambda _: None)


def test_player_options_labels_have_readable_contrast(qtbot, tmp_path, package_inputs):
    from PySide6.QtCore import QTimer
    from PySide6.QtGui import QPalette
    from PySide6.QtWidgets import QApplication, QLabel

    from rominabox.ui.player import Player

    kit, _ = package_inputs
    player = Player(create_project(tmp_path), kit, tmp_path / "data")
    qtbot.addWidget(player)
    player.show()
    colors = []

    def capture():
        dialog = QApplication.activeModalWidget()
        label = dialog.findChildren(QLabel)[0]
        colors.append(
            (label.palette().color(QPalette.ColorRole.WindowText), dialog.palette().color(QPalette.ColorRole.Window))
        )
        dialog.reject()

    QTimer.singleShot(50, capture)
    player.options()

    def luminance(color):
        values = [color.redF(), color.greenF(), color.blueF()]
        linear = [v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4 for v in values]
        return sum(a * b for a, b in zip(linear, [0.2126, 0.7152, 0.0722], strict=True))

    foreground, background = map(luminance, colors[0])
    assert (max(foreground, background) + 0.05) / (min(foreground, background) + 0.05) >= 4.5


def test_packaged_menu_smoke_does_not_touch_player_saves(tmp_path):
    project = create_project(tmp_path)
    project.write(tmp_path / "player.json")
    code = """
from pathlib import Path
from unittest.mock import patch
from rominabox.ui import app
import sys
root = Path(sys.argv[1])
app.resources = lambda: root
with patch('rominabox.ui.player.data_home', side_effect=RuntimeError('Touched real player saves')):
    raise SystemExit(app.run(smoke_output=root / 'preview.png'))
"""
    result = subprocess.run([sys.executable, "-c", code, str(tmp_path)], capture_output=True, text=True, timeout=3)
    assert result.returncode == 0, result.stderr
    assert (tmp_path / "preview.png").is_file()


def test_builder_or_wrong_theme_template_cannot_be_exported(tmp_path, package_inputs):
    kit, template = package_inputs
    project = create_project(tmp_path)
    marker = template / "Contents/Resources/player-template.json"
    marker.parent.mkdir(parents=True, exist_ok=True)
    for metadata in (None, {"kind": "builder", "theme": "8-bit"}, {"kind": "player", "theme": "other"}):
        if metadata is None:
            marker.unlink(missing_ok=True)
        else:
            marker.write_text(json.dumps(metadata))
        with pytest.raises(AppError, match="player template"):
            packaging.build(project, tmp_path / "out", kit, template, signer=lambda _: None)
    assert not (tmp_path / "out").exists()


def test_export_copies_only_selected_runtime_material(tmp_path, package_inputs):
    kit, template = package_inputs
    for relative in (
        "catalogs/unrelated.dat",
        "headless/retroarch",
        "sources/unrelated.tar.gz",
        "licenses/unrelated.txt",
        "info/unrelated_libretro.info",
        "info/core_info.cache",
        "cores/unrelated_libretro.dylib",
    ):
        path = kit / relative
        path.parent.mkdir(exist_ok=True)
        path.write_bytes(b"must not ship in this game")
    info = kit / "info/gambatte_libretro.info"
    info.write_text('savestate = "true"')
    output = packaging.build(create_project(tmp_path), tmp_path / "out", kit, template, signer=lambda _: None)
    bundled = output / "Contents/Resources/runtime-kit"
    assert not (bundled / "catalogs").exists()
    assert not (bundled / "headless").exists()
    assert {p.name for p in (bundled / "sources").iterdir()} == {"gambatte.tar.gz", "RetroArch.tar.gz"}
    assert {p.name for p in (bundled / "licenses").iterdir()} == {"gambatte.txt", "RetroArch.txt"}
    assert {p.name for p in (bundled / "info").iterdir()} == {"gambatte_libretro.info"}
    assert {p.name for p in (bundled / "cores").iterdir()} == {"gambatte_libretro.dylib"}
    assert (bundled / "RetroArch.app/Contents/MacOS/RetroArch").is_file()
