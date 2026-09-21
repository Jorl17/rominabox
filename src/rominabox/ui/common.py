from __future__ import annotations

from collections.abc import Callable
from pathlib import Path

from PySide6.QtCore import QThread, Signal
from PySide6.QtGui import QDragEnterEvent, QDropEvent
from PySide6.QtWidgets import QFileDialog, QPushButton, QWidget


class Job(QThread):
    """Run one blocking operation off the UI thread and deliver its result or error."""

    result = Signal(object)
    failed = Signal(str)
    progress = Signal(str)

    def __init__(self, work: Callable[[], object], parent: QWidget) -> None:
        super().__init__(parent)
        self.work = work

    def run(self) -> None:
        try:
            self.result.emit(self.work())
        except Exception as exc:
            self.failed.emit(str(exc))


class FileButton(QPushButton):
    """A conventional file chooser that also accepts a single dropped local file."""

    selected = Signal(object)

    def __init__(self, text: str, file_filter: str = "All files (*)") -> None:
        super().__init__(text)
        self.file_filter = file_filter
        self.setAcceptDrops(True)
        self.clicked.connect(self.choose)

    def choose(self) -> None:
        path, _ = QFileDialog.getOpenFileName(self, "Choose file", "", self.file_filter)
        if path:
            self.selected.emit(Path(path))

    def dragEnterEvent(self, event: QDragEnterEvent) -> None:
        urls = event.mimeData().urls()
        if len(urls) == 1 and urls[0].isLocalFile():
            event.acceptProposedAction()

    def dropEvent(self, event: QDropEvent) -> None:
        urls = event.mimeData().urls()
        if len(urls) == 1 and urls[0].isLocalFile():
            self.selected.emit(Path(urls[0].toLocalFile()))
            event.acceptProposedAction()
