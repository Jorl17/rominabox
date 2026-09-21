import json
import subprocess
import sys

import pytest
from PIL import Image
from pydantic import ValidationError
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QComboBox, QLabel, QPushButton, QVBoxLayout

from rominabox.cli import main
from rominabox.domain import Project
from rominabox.menu import MenuAppearance, MenuState, SlotStatus
from rominabox.themes import THEMES, Theme
from rominabox.ui.builder import Builder
from rominabox.ui.menu import MenuView, create_menu
from rominabox.ui.player import Player


class ListMenu(MenuView):
    """Independent test renderer: one selected slot, rather than the production card grid."""

    def __init__(self, appearance, parent=None):
        super().__init__(appearance, parent)
        self.resize(500, 400)
        layout = QVBoxLayout(self)
        self.heading = QLabel()
        self.heading.setStyleSheet("background: #e61723; color: #ffffff;")
        layout.addWidget(self.heading)
        self.selection = QComboBox()
        self.selection.addItems([str(n) for n in range(1, 7)])
        layout.addWidget(self.selection)
        self.save = QPushButton("Save selected slot")
        self.save.setObjectName("saveSlot1")
        self.save.clicked.connect(lambda: self.save_requested.emit(self.selection.currentIndex() + 1))
        layout.addWidget(self.save)
        self.load = QPushButton("Load selected slot")
        self.load.setObjectName("loadSlot1")
        self.load.clicked.connect(lambda: self.load_requested.emit(self.selection.currentIndex() + 1))
        layout.addWidget(self.load)
        footer = QLabel("Footer")
        footer.setFixedHeight(20)
        footer.setStyleSheet("background: #17a2b8;")
        layout.addWidget(footer)

    def present(self, state):
        self.heading.setText(state.title)
        self.save.setEnabled(state.can_resume)
        self.load.setEnabled(state.slots[self.selection.currentIndex()].status == SlotStatus.SAVED)
        for slot in state.slots:
            self.selection.setItemText(slot.number - 1, f"{slot.number}: {slot.status}")


@pytest.fixture
def alternate_theme(monkeypatch):
    theme = Theme("test-list", "Test list", __name__, "ListMenu")
    monkeypatch.setitem(THEMES, theme.id, theme)
    return theme.id


def project_for(tmp_path):
    rom = tmp_path / "theme-test.gbc"
    rom.write_bytes(bytes(512))
    return Project.from_rom(rom)


def test_old_projects_keep_the_design_and_unknown_themes_fail(tmp_path):
    project = project_for(tmp_path)
    data = project.model_dump(mode="json")
    del data["theme"]
    assert Project.model_validate(data).theme == "8-bit"
    data["theme"] = "somewhere.arbitrary:Code"
    with pytest.raises(ValidationError, match="Choose a theme"):
        Project.model_validate(data)


def test_theme_discovery_does_not_load_qt_or_need_a_runtime():
    result = subprocess.run(
        [
            sys.executable,
            "-c",
            "from rominabox.cli import main; import sys; main(['themes']); assert 'PySide6' not in sys.modules",
        ],
        text=True,
        capture_output=True,
    )
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout) == [{"id": "8-bit", "name": "8-bit", "default": True}]


def test_eight_bit_controls_emit_intent(qtbot):
    view = create_menu("8-bit", MenuAppearance("#1647d8"))
    qtbot.addWidget(view)
    view.present(MenuState("Test"))
    view.show()
    for label, signal in (
        ("PLAY", view.play_requested),
        ("OPTIONS", view.options_requested),
        ("QUIT", view.quit_requested),
    ):
        button = next(b for b in view.findChildren(QPushButton) if b.text() == label)
        with qtbot.waitSignal(signal, timeout=300):
            qtbot.mouseClick(button, Qt.MouseButton.LeftButton)


def test_theme_selection_flows_through_cli_builder_preview_and_package(
    qtbot, tmp_path, package_inputs, alternate_theme, monkeypatch, capsys
):
    kit, template = package_inputs
    project = project_for(tmp_path)
    assert main(["inspect", str(project.rom), "--theme", alternate_theme]) == 0
    selected = Project.model_validate_json(capsys.readouterr().out)
    assert selected.theme == alternate_theme
    selected.write(tmp_path / "project.json")
    assert Project.read(tmp_path / "project.json").theme == alternate_theme
    assert main(["themes"]) == 0
    assert alternate_theme in {t["id"] for t in json.loads(capsys.readouterr().out)}

    (template / "Contents/Resources/player-template.json").write_text(
        json.dumps({"schema_version": 1, "kind": "player", "theme": alternate_theme})
    )
    widget = Builder(kit, template)
    qtbot.addWidget(widget)
    widget.lookup.setChecked(False)
    widget.select_rom(project.rom)
    qtbot.waitUntil(lambda: widget.project is not None and not widget.job.isRunning())
    widget.theme.setCurrentIndex(widget.theme.findData(alternate_theme))
    widget.preview_menu()
    qtbot.addWidget(widget.player_preview)
    assert isinstance(widget.player_preview, ListMenu)
    assert widget.player_preview.heading.text() == project.title

    # The only substituted packaging boundary is the external platform signer.
    monkeypatch.setattr(subprocess, "run", lambda *a, **kw: subprocess.CompletedProcess(a, 0, "", ""))
    widget.start_build(tmp_path / "out")
    qtbot.waitUntil(lambda: widget.output is not None and not widget.job.isRunning())
    exported = Project.read(widget.output / "Contents/Resources/player.json")
    assert exported.theme == alternate_theme
    assert exported.game_id == project.game_id
    player = Player(exported, kit, tmp_path / "data")
    qtbot.addWidget(player)
    assert isinstance(player.view, ListMenu)
    assert player.view.heading.text() == project.title


@pytest.mark.parametrize("first,second", [("8-bit", "test-list"), ("test-list", "8-bit")])
def test_save_and_load_work_across_independent_layouts(
    qtbot, tmp_path, package_inputs, alternate_theme, monkeypatch, first, second
):
    kit, _ = package_inputs
    # Supply both platform layouts at the external-process boundary. We launch no emulator.
    windows_runtime = kit / "RetroArch/retroarch.exe"
    windows_runtime.parent.mkdir()
    windows_runtime.write_bytes(b"external runtime fixture")
    (kit / "cores/gambatte_libretro.dll").write_bytes(b"external core fixture")
    project = project_for(tmp_path)
    project.theme = first
    home = tmp_path / "saves"
    # Seed an external emulator checkpoint. Menu state and save storage are not substituted.
    home.joinpath("states").mkdir(parents=True)
    checkpoint = home / "states/game.state.auto"
    checkpoint.write_bytes(b"checkpoint to keep")
    Image.new("RGB", (160, 144), "#ff0000").save(checkpoint.with_name(checkpoint.name + ".png"))
    player = Player(project, kit, home)
    qtbot.addWidget(player)
    player.show()
    qtbot.mouseClick(player.view.findChild(QPushButton, "saveSlot1"), Qt.MouseButton.LeftButton)
    assert player.session.store.slot(1) is not None
    player.close()

    project.theme = second
    checkpoint.write_bytes(b"different progress")
    resumed = Player(project, kit, home)
    qtbot.addWidget(resumed)
    resumed.show()
    commands = []

    class ExternalProcess:
        returncode = None

        def __init__(self, command, **kwargs):
            commands.append(command)

        def poll(self):
            return self.returncode

    monkeypatch.setattr(subprocess, "Popen", ExternalProcess)
    qtbot.mouseClick(resumed.view.findChild(QPushButton, "loadSlot1"), Qt.MouseButton.LeftButton)
    assert checkpoint.read_bytes() == b"checkpoint to keep"
    assert resumed.session.store.preview(1).is_file()
    assert len(commands) == 1
    assert 'savestate_auto_load = "true"' in (home / "session.cfg").read_text()
    # Complete the external process boundary so that the controller's logs close and its timer stops.
    resumed.session.process.returncode = 0
    checkpoint.write_bytes(b"resumed progress")
    resumed.poll()
    assert resumed.can_save
    assert not resumed.timer.isActive()


def test_menu_keeps_qt_native_render_available(qtbot):
    from PySide6.QtGui import QPixmap

    view = create_menu("8-bit", MenuAppearance("#1647d8"))
    qtbot.addWidget(view)
    canvas = QPixmap(view.size())
    # Qt painting and export callers use QWidget.render, so we must not replace its contract in a theme.
    view.render(canvas)
    assert not canvas.isNull()


def test_embedded_theme_retains_its_background(qtbot, tmp_path, package_inputs):
    kit, _ = package_inputs
    player = Player(project_for(tmp_path), kit, tmp_path / "data")
    qtbot.addWidget(player)
    player.show()
    assert player.grab().toImage().pixelColor(0, 0).name() == "#ffffff"


def test_small_builder_preview_keeps_top_and_bottom_of_theme(qtbot, package_inputs, alternate_theme):
    kit, template = package_inputs
    builder = Builder(kit, template)
    qtbot.addWidget(builder)
    builder.theme.setCurrentIndex(builder.theme.findData(alternate_theme))
    builder.pages.setCurrentIndex(1)
    builder.resize(740, 680)
    builder.show()
    frame = builder.preview.grab().toImage()
    colours = {frame.pixelColor(x, y).name() for y in range(frame.height()) for x in range(frame.width())}
    assert "#e61723" in colours, "The theme header must remain visible"
    assert "#17a2b8" in colours, "The theme footer must remain visible"
