from __future__ import annotations

import sys
from pathlib import Path

from PySide6.QtCore import QTimer, QUrl
from PySide6.QtGui import QCloseEvent, QDesktopServices, QKeySequence, QShortcut
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from ..domain import VERSION, AppError, Project
from ..menu import MENU_HINT, MenuAppearance, read_menu_state
from ..paths import data_home
from ..runtime import DEFAULT_KEYS, RuntimeSession
from .menu import create_menu


class Player(QWidget):
    """Manage the visible menu and the lifetime of one separate RetroArch session."""

    def __init__(self, project: Project, kit: Path, home: Path | None = None) -> None:
        super().__init__()
        self.project = project
        self.session = RuntimeSession(project, kit, home or data_home() / "games" / project.game_id)
        self.can_save = self.session.store.checkpoint.is_file() and project.autosave
        self.setWindowTitle(project.title)
        self.view = create_menu(project.theme, MenuAppearance(project.accent, project.background), self)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(self.view)
        self.resize(self.view.size())
        self.view.play_requested.connect(lambda: self.launch(self.can_save))
        self.view.fresh_requested.connect(self.start_fresh)
        self.view.save_requested.connect(self.save_slot)
        self.view.load_requested.connect(self.load_slot)
        self.view.options_requested.connect(self.options)
        self.view.quit_requested.connect(self.close)
        self.view.about_requested.connect(self.about)
        self.timer = QTimer(self)
        self.timer.setInterval(200)
        self.timer.timeout.connect(self.poll)
        self.shortcut = QShortcut(QKeySequence("Ctrl+Meta+F" if sys.platform == "darwin" else "Alt+Return"), self)
        self.shortcut.activated.connect(self.toggle_fullscreen)
        self.refresh()
        if project.fullscreen:
            self.showFullScreen()
        if not project.show_menu:
            QTimer.singleShot(0, lambda: self.launch(self.can_save))

    def refresh(self, status: str = MENU_HINT) -> None:
        self.view.present(read_menu_state(self.project.title, self.session.store, self.can_save, status))

    def launch(self, resume: bool) -> None:
        try:
            self.session.start(resume)
        except (AppError, OSError) as exc:
            QMessageBox.warning(self, "Could not start game", str(exc))
            return
        self.hide()
        self.timer.start()

    def poll(self) -> None:
        if self.session.process is None or self.session.process.poll() is None:
            return
        self.timer.stop()
        self.can_save = self.session.finish()
        status = MENU_HINT if self.can_save else "No new checkpoint was saved. Existing manual slots are safe."
        self.refresh(status)
        self.show()
        self.raise_()
        self.activateWindow()

    def save_slot(self, number: int) -> None:
        if not self.can_save:
            return
        try:
            if (
                self.session.store.slot(number)
                and QMessageBox.question(self, "Replace save", f"Replace slot {number}?")
                != QMessageBox.StandardButton.Yes
            ):
                return
            self.session.store.save(number)
            self.refresh()
        except (AppError, OSError) as exc:
            QMessageBox.warning(self, "Could not save", str(exc))

    def load_slot(self, number: int) -> None:
        try:
            self.session.store.restore(number)
            self.can_save = True
            self.launch(True)
        except (AppError, OSError) as exc:
            QMessageBox.warning(self, "Could not load", str(exc))

    def start_fresh(self) -> None:
        if (
            QMessageBox.question(self, "Start fresh", "Start from the beginning? Manual slots will remain available.")
            == QMessageBox.StandardButton.Yes
        ):
            self.launch(False)

    def toggle_fullscreen(self) -> None:
        self.showNormal() if self.isFullScreen() else self.showFullScreen()

    def options(self) -> None:
        dialog = QDialog(self)
        dialog.setWindowTitle("Options")
        form = QFormLayout(dialog)
        fullscreen = QCheckBox()
        fullscreen.setChecked(bool(self.session.settings["fullscreen"]))
        form.addRow("Fullscreen", fullscreen)
        filter_box = QComboBox()
        filter_box.addItem("Sharp pixels", "clean")
        filter_box.addItem("Smooth", "smooth")
        filter_box.setCurrentIndex(max(0, filter_box.findData(self.session.settings["shader"])))
        form.addRow("Filter", filter_box)
        fields: dict[str, QLineEdit] = {}
        saved_keys = self.session.settings.get("keys", DEFAULT_KEYS)
        for button, key in DEFAULT_KEYS.items():
            field = QLineEdit(str(saved_keys.get(button, key)) if isinstance(saved_keys, dict) else key)
            field.setMaximumWidth(140)
            field.setToolTip("RetroArch key name, e.g. x, enter, space, rshift. Changes apply when the game starts.")
            fields[button] = field
            form.addRow(button.capitalize(), field)
        form.addRow(QLabel("F1 in game opens RetroArch settings."))
        logs = QPushButton("Open logs")
        logs.clicked.connect(lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(str(self.session.home / "logs"))))
        form.addRow(logs)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Save | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        form.addRow(buttons)
        if dialog.exec() == QDialog.DialogCode.Accepted:
            self.session.settings.update(
                fullscreen=fullscreen.isChecked(),
                shader=filter_box.currentData(),
                keys={button: field.text().strip() for button, field in fields.items()},
            )
            self.session.save_preferences()

    def about(self) -> None:
        QMessageBox.information(
            self,
            "About",
            f"{self.project.title}\n{self.project.description}\n\nROM-in-a-Box {VERSION}\n"
            "RetroArch 1.22.2 · Local prototype\n\nGame menu: Esc or Start + Select\n"
            "RetroArch menu: F1 or L3 + R3\n\nReturning to this menu checkpoints and restarts the emulator.",
        )

    def closeEvent(self, event: QCloseEvent) -> None:
        if self.timer.isActive():
            event.ignore()
            return
        event.accept()
