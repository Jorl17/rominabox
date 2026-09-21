from PySide6.QtCore import QRect, QSize, Qt
from PySide6.QtGui import QPainter, QPaintEvent, QPixmap
from PySide6.QtWidgets import QWidget

from ..menu import MenuAppearance, MenuState
from .menu import create_menu


class MenuPreview(QWidget):
    """Fit a snapshot from the real theme renderer into the builder without cropping or stretching."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._image = QPixmap()
        self.setMinimumSize(160, 140)

    def set_menu(self, theme_id: str, appearance: MenuAppearance, state: MenuState) -> None:
        """Render a passive preview with no runtime, storage, or connected action handlers."""
        view = create_menu(theme_id, appearance)
        try:
            view.present(state)
            self._image = view.grab()
        finally:
            view.deleteLater()
        self.update()

    def sizeHint(self) -> QSize:
        return QSize(560, 320)

    def paintEvent(self, event: QPaintEvent) -> None:
        if self._image.isNull():
            return
        size = self._image.deviceIndependentSize().toSize().scaled(self.size(), Qt.AspectRatioMode.KeepAspectRatio)
        target = QRect(
            (self.width() - size.width()) // 2, (self.height() - size.height()) // 2, size.width(), size.height()
        )
        painter = QPainter(self)
        painter.drawPixmap(target, self._image)
