from __future__ import annotations

from pathlib import Path
from typing import cast

from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QCloseEvent, QDesktopServices, QPixmap
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QFileDialog,
    QFormLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QProgressBar,
    QPushButton,
    QStackedWidget,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from .. import packaging
from ..catalog import enrich
from ..domain import SYSTEMS, VERSION, AppError, Project
from ..menu import MenuAppearance, MenuState
from ..paths import runtime_kit
from ..themes import THEMES
from .common import FileButton, Job
from .menu import create_menu
from .preview import MenuPreview


class Builder(QWidget):
    """Present the shared authoring project in three short steps, with asynchronous lookup and builds."""

    def __init__(self, kit: Path | None = None, template: Path | None = None) -> None:
        super().__init__()
        self.kit = kit or runtime_kit()
        self.template = template
        self.project: Project | None = None
        self.rom_path: Path | None = None
        self.job: Job | None = None
        self.cancel_requested = False
        self.output: Path | None = None
        self.setWindowTitle("ROM-in-a-Box")
        self.resize(740, 680)
        self.setMinimumSize(620, 580)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(32, 26, 32, 24)
        layout.setSpacing(18)
        heading = QLabel("ROM-in-a-Box")
        heading.setStyleSheet("font-size: 26px; font-weight: 600;")
        layout.addWidget(heading)
        self.steps = QLabel("1  Game     /     2  Menu     /     3  Build")
        layout.addWidget(self.steps)
        self.pages = QStackedWidget()
        layout.addWidget(self.pages, 1)
        self.game_page()
        self.menu_page()
        self.build_page()
        self.status = QLabel("")
        self.status.setWordWrap(True)
        layout.addWidget(self.status)
        navigation = QHBoxLayout()
        self.back = QPushButton("Back")
        self.back.clicked.connect(self.previous)
        self.back.setEnabled(False)
        navigation.addWidget(self.back)
        navigation.addStretch()
        self.next = QPushButton("Next")
        self.next.setDefault(True)
        self.next.setEnabled(False)
        self.next.clicked.connect(self.advance)
        navigation.addWidget(self.next)
        layout.addLayout(navigation)
        version = QLabel(f"{VERSION} · macOS prototype")
        version.setAlignment(Qt.AlignmentFlag.AlignRight)
        layout.addWidget(version)

    def game_page(self) -> None:
        page = QWidget()
        box = QVBoxLayout(page)
        self.drop = FileButton("Drop a game here, or choose a file")
        self.drop.setMinimumHeight(110)
        self.drop.selected.connect(self.select_rom)
        box.addWidget(self.drop)
        self.lookup = QCheckBox("Look up game details")
        self.lookup.setChecked(True)
        self.lookup.setToolTip(
            "Match against local catalogs and fetch matching artwork. Your game file is never uploaded."
        )
        box.addWidget(self.lookup)
        form = QFormLayout()
        self.title = QLineEdit()
        form.addRow("Name", self.title)
        self.system = QComboBox()
        self.system.addItem("Automatic", None)
        available = packaging.available_systems(self.kit)
        for system in SYSTEMS.values():
            suffix = "" if system.id in available else " — core not bundled"
            self.system.addItem(system.name + suffix, system.id)
        self.system.currentIndexChanged.connect(self.system_changed)
        form.addRow("System", self.system)
        self.description = QTextEdit()
        self.description.setMaximumHeight(75)
        form.addRow("Description", self.description)
        self.icon = FileButton("Choose icon…", "Images (*.png *.jpg *.jpeg *.webp *.bmp *.ico)")
        self.icon.selected.connect(self.select_icon)
        form.addRow("Icon", self.icon)
        box.addLayout(form)
        self.art_preview = QLabel()
        self.art_preview.setAlignment(Qt.AlignmentFlag.AlignCenter)
        box.addWidget(self.art_preview)
        box.addStretch()
        self.pages.addWidget(page)

    def menu_page(self) -> None:
        page = QWidget()
        box = QVBoxLayout(page)
        self.menu = QCheckBox("Show menu at startup")
        self.menu.setChecked(True)
        self.menu.setToolTip("When off, start the game directly. Esc returns to the game menu.")
        self.fullscreen = QCheckBox("Start fullscreen")
        self.autosave = QCheckBox("Offer Continue at next launch")
        self.autosave.setChecked(True)
        for field in (self.menu, self.fullscreen, self.autosave):
            box.addWidget(field)
        form = QFormLayout()
        self.theme = QComboBox()
        for theme in THEMES.values():
            self.theme.addItem(theme.name, theme.id)
        self.theme.currentIndexChanged.connect(self.update_preview)
        form.addRow("Theme", self.theme)
        self.filter = QComboBox()
        self.filter.addItem("Sharp pixels", "clean")
        self.filter.addItem("Smooth", "smooth")
        form.addRow("Filter", self.filter)
        self.background = FileButton("Choose background…", "Images (*.png *.jpg *.jpeg *.webp *.bmp)")
        self.background.selected.connect(self.select_background)
        form.addRow("Background", self.background)
        self.accent = QComboBox()
        for title, color in [("Blue", "#1647d8"), ("Red", "#c51b2b"), ("Green", "#00683d"), ("Black", "#111c40")]:
            self.accent.addItem(title, color)
        self.accent.currentIndexChanged.connect(self.update_preview)
        form.addRow("Menu colour", self.accent)
        box.addLayout(form)
        self.preview = MenuPreview()
        box.addWidget(self.preview, 1)
        preview_button = QPushButton("Preview menu")
        preview_button.clicked.connect(self.preview_menu)
        box.addWidget(preview_button)
        self.pages.addWidget(page)
        self.update_preview()

    def build_page(self) -> None:
        page = QWidget()
        box = QVBoxLayout(page)
        self.summary = QLabel()
        self.summary.setWordWrap(True)
        self.summary.setStyleSheet("font-size: 18px;")
        box.addWidget(self.summary)
        box.addStretch()
        save_project = QPushButton("Save project…")
        save_project.clicked.connect(self.export_project)
        box.addWidget(save_project)
        self.progress = QProgressBar()
        self.progress.hide()
        box.addWidget(self.progress)
        self.cancel = QPushButton("Cancel build")
        self.cancel.clicked.connect(self.cancel_build)
        self.cancel.hide()
        box.addWidget(self.cancel)
        self.reveal = QPushButton("Show in Finder")
        self.reveal.clicked.connect(self.reveal_output)
        self.reveal.hide()
        box.addWidget(self.reveal)
        box.addStretch()
        self.pages.addWidget(page)

    def select_rom(self, path: Path, system: str | None = None) -> None:
        if self.job and self.job.isRunning():
            return
        self.rom_path = path
        self.status.setText("Reading game…")
        self.next.setEnabled(False)
        self.drop.setEnabled(False)
        lookup = self.lookup.isChecked()

        def inspect() -> object:
            project = Project.from_rom(path, system)
            return enrich(project, self.kit) if lookup else (project, "From game header / filename")

        self.job = Job(inspect, self)
        self.job.result.connect(self.inspected)
        self.job.failed.connect(self.failed)
        self.job.start()

    def inspected(self, result: object) -> None:
        project, detail = cast(tuple[Project, str], result)
        self.project = project
        self.title.setText(project.title)
        self.system.blockSignals(True)
        self.system.setCurrentIndex(self.system.findData(project.system))
        self.system.blockSignals(False)
        self.drop.setText(project.rom.name)
        self.drop.setEnabled(True)
        self.status.setText(detail)
        self.next.setEnabled(True)
        if project.icon:
            self.select_icon(project.icon)

    def system_changed(self) -> None:
        if self.rom_path and (not self.job or not self.job.isRunning()):
            self.select_rom(self.rom_path, self.system.currentData())

    def select_icon(self, path: Path) -> None:
        if self.project:
            self.project.icon = path
            self.icon.setText(path.name)
            self.art_preview.setPixmap(
                QPixmap(str(path)).scaled(
                    100, 100, Qt.AspectRatioMode.KeepAspectRatio, Qt.TransformationMode.SmoothTransformation
                )
            )

    def select_background(self, path: Path) -> None:
        if self.project:
            self.project.background = path
            self.background.setText(path.name)
            self.update_preview()

    def update_preview(self) -> None:
        title = self.title.text() or "Your game"
        self.preview.set_menu(
            self.theme.currentData(),
            MenuAppearance(self.accent.currentData(), self.project.background if self.project else None),
            MenuState(title),
        )

    def collect(self) -> Project:
        if self.project is None:
            raise AppError("rom_missing", "Choose a game first.")
        data = self.project.model_dump()
        data.update(
            title=self.title.text(),
            description=self.description.toPlainText(),
            theme=self.theme.currentData(),
            show_menu=self.menu.isChecked(),
            fullscreen=self.fullscreen.isChecked(),
            autosave=self.autosave.isChecked(),
            shader=self.filter.currentData(),
            accent=self.accent.currentData(),
        )
        return Project.model_validate(data)

    def advance(self) -> None:
        try:
            self.project = self.collect()
            index = self.pages.currentIndex()
            if index == 2:
                directory = QFileDialog.getExistingDirectory(self, "Build game app in…")
                if directory:
                    self.start_build(Path(directory))
                return
            self.pages.setCurrentIndex(index + 1)
            self.back.setEnabled(True)
            self.next.setText("Build app…" if index == 1 else "Next")
            self.status.clear()
            self.summary.setText(f"{self.project.title}\n{SYSTEMS[self.project.system].name}\n\nmacOS · Standalone app")
            self.update_preview()
        except (AppError, ValueError) as exc:
            self.failed(str(exc))

    def previous(self) -> None:
        index = max(0, self.pages.currentIndex() - 1)
        self.pages.setCurrentIndex(index)
        self.back.setEnabled(index > 0)
        self.next.setText("Next")

    def start_build(self, output: Path) -> None:
        project = self.collect()
        self.cancel_requested = False
        self.next.setEnabled(False)
        self.back.setEnabled(False)
        self.pages.setEnabled(False)
        self.progress.setRange(0, 0)
        self.progress.show()
        self.pages.setEnabled(True)
        self.cancel.show()

        def progress(message: str) -> None:
            if self.cancel_requested:
                raise AppError("cancelled", "Build cancelled.")
            if self.job:
                self.job.progress.emit(message)

        self.job = Job(lambda: packaging.build(project, output, self.kit, self.template, progress), self)
        self.job.progress.connect(self.status.setText)
        self.job.result.connect(self.built)
        self.job.failed.connect(self.failed)
        self.job.start()

    def cancel_build(self) -> None:
        self.cancel_requested = True
        self.status.setText("Cancelling after the current step…")

    def built(self, result: object) -> None:
        self.output = Path(str(result))
        self.progress.hide()
        self.cancel.hide()
        self.reveal.show()
        self.status.setText("Your app is ready.")
        self.next.setEnabled(True)
        self.back.setEnabled(True)

    def failed(self, message: str) -> None:
        self.status.setText(message)
        self.drop.setEnabled(True)
        self.next.setEnabled(self.project is not None)
        self.back.setEnabled(self.pages.currentIndex() > 0)
        self.progress.hide()
        self.cancel.hide()

    def export_project(self) -> None:
        try:
            project = self.collect()
            filename, _ = QFileDialog.getSaveFileName(self, "Save project", f"{project.title}.json", "Project (*.json)")
            if filename:
                project.write(Path(filename))
        except (AppError, ValueError, OSError) as exc:
            self.failed(str(exc))

    def reveal_output(self) -> None:
        if self.output:
            QDesktopServices.openUrl(QUrl.fromLocalFile(str(self.output.parent)))

    def preview_menu(self) -> None:
        """Preview the selected renderer without creating a runtime or accessing player saves."""
        try:
            project = self.collect()
            self.player_preview = create_menu(project.theme, MenuAppearance(project.accent, project.background))
            self.player_preview.setWindowTitle(f"Preview — {project.title}")
            self.player_preview.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
            self.player_preview.present(MenuState(project.title))
            self.player_preview.quit_requested.connect(self.player_preview.close)
            self.player_preview.show()
        except (AppError, ValueError) as exc:
            self.failed(str(exc))

    def closeEvent(self, event: QCloseEvent) -> None:
        if self.job and self.job.isRunning():
            self.cancel_requested = True
            self.status.setText("Please wait for the current operation to finish.")
            event.ignore()
        else:
            event.accept()
