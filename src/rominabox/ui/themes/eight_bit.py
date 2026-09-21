from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QFontDatabase, QPixmap
from PySide6.QtWidgets import (
    QGraphicsDropShadowEffect,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from ...menu import MenuAppearance, MenuState, SlotStatus
from ..menu import MenuView


class EightBitMenu(MenuView):
    """The bright, square-framed menu, kept as an independent 8-bit design."""

    def __init__(self, appearance: MenuAppearance, parent: QWidget | None = None) -> None:
        super().__init__(appearance, parent)
        self.resize(880, 720)
        self.setMinimumSize(760, 650)
        self.setObjectName("eightBitMenu")
        self.setFont(QFontDatabase.systemFont(QFontDatabase.SystemFont.FixedFont))
        background = ""
        if appearance.background:
            background = f'border-image: url("{appearance.background.as_posix()}") 0 0 0 0 stretch stretch;'
        self.setStyleSheet(f"""
            QWidget#eightBitMenu {{background: #ffffff; {background}}}
            QDialog, QMessageBox {{background: #ffffff;}}
            QLabel {{color: #111c40; background: transparent;}}
            QLabel#gameTitle {{background: {appearance.accent}; color: #ffffff;
                               border: 3px solid #111c40; padding: 12px; font-size: 28px; font-weight: bold;}}
            QPushButton {{background: #ffffff; color: #111c40; border: 3px solid #111c40;
                          border-radius: 0px; padding: 9px 16px; font-weight: bold;}}
            QPushButton:hover, QPushButton:focus {{background: #ffe52b; border-color: {appearance.accent};}}
            QPushButton:pressed {{background: {appearance.accent}; color: #ffffff;}}
            QPushButton:disabled {{color: #59627a; border-color: #8a91a3; background: #ffffff;}}
            QPushButton#primary {{background: #ffe52b; color: #111c40;}}
            QPushButton#primary:focus, QPushButton#primary:hover {{background: {appearance.accent}; color: #ffffff;}}
            QWidget#slot {{background: #ffffff; border: 3px solid #111c40; border-radius: 0px;}}
            QLabel#slotHeading {{background: {appearance.accent}; color: #ffffff;
                                 padding: 6px; font-size: 16px; font-weight: bold;}}
            QLabel#slotPreview {{border: 2px solid {appearance.accent}; font-weight: bold; color: {appearance.accent};}}
        """)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(32, 26, 32, 22)
        layout.setSpacing(18)
        title = QLabel()
        self.title = title
        title.setWordWrap(True)
        title.setObjectName("gameTitle")
        self.add_shadow(title)
        layout.addWidget(title)
        self.status = QLabel("Esc · Game menu     F1 · RetroArch options")
        layout.addWidget(self.status)
        actions = QHBoxLayout()
        self.play = QPushButton("PLAY")
        self.play.setObjectName("primary")
        self.play.clicked.connect(self.play_requested.emit)
        self.fresh = QPushButton("START FRESH")
        self.fresh.clicked.connect(self.fresh_requested.emit)
        options = QPushButton("OPTIONS")
        options.clicked.connect(self.options_requested.emit)
        for button in (self.play, self.fresh, options):
            self.add_shadow(button)
            actions.addWidget(button)
        layout.addLayout(actions)
        self.grid = QGridLayout()
        self.grid.setSpacing(12)
        layout.addLayout(self.grid, 1)
        footer = QHBoxLayout()
        quit_button = QPushButton("QUIT")
        quit_button.clicked.connect(self.quit_requested.emit)
        footer.addWidget(quit_button)
        footer.addStretch()
        version = QPushButton()
        self.version = version
        version.setFlat(True)
        version.clicked.connect(self.about_requested.emit)
        footer.addWidget(version)
        layout.addLayout(footer)

    @staticmethod
    def add_shadow(widget: QWidget) -> None:
        """Give menu elements a solid offset silhouette without blur."""
        effect = QGraphicsDropShadowEffect(widget)
        effect.setBlurRadius(0)
        effect.setOffset(3, 3)
        effect.setColor(QColor("#111c40"))
        widget.setGraphicsEffect(effect)

    def present(self, state: MenuState) -> None:
        self.title.setText(state.title.upper())
        self.status.setText(state.status)
        self.version.setText(f"ROM-in-a-Box {state.version}")
        while self.grid.count():
            item = self.grid.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        self.play.setText("CONTINUE" if state.can_resume else "PLAY")
        self.fresh.setVisible(state.can_resume)
        for slot in state.slots:
            number = slot.number
            card = QWidget()
            card.setObjectName("slot")
            self.add_shadow(card)
            box = QVBoxLayout(card)
            heading = QLabel(f"SLOT {number:02}")
            heading.setObjectName("slotHeading")
            box.addWidget(heading)
            preview = QLabel()
            preview.setObjectName("slotPreview")
            preview.setAlignment(Qt.AlignmentFlag.AlignCenter)
            preview.setMinimumHeight(118)
            if slot.preview:
                preview.setPixmap(
                    QPixmap(str(slot.preview)).scaled(
                        208, 130, Qt.AspectRatioMode.KeepAspectRatio, Qt.TransformationMode.FastTransformation
                    )
                )
            else:
                preview.setText(
                    {SlotStatus.EMPTY: "EMPTY", SlotStatus.SAVED: "NO PREVIEW", SlotStatus.DAMAGED: "Damaged save"}[
                        slot.status
                    ]
                )
            timestamp = slot.saved_at.strftime("%d %b · %H:%M") if slot.saved_at else ""
            box.addWidget(preview, 1)
            if timestamp:
                box.addWidget(QLabel(timestamp))
            buttons = QHBoxLayout()
            save = QPushButton("Save")
            save.setObjectName(f"saveSlot{number}")
            save.setEnabled(state.can_resume)
            save.clicked.connect(lambda checked=False, n=number: self.save_requested.emit(n))
            load = QPushButton("Load")
            load.setObjectName(f"loadSlot{number}")
            load.setEnabled(slot.status == SlotStatus.SAVED)
            load.clicked.connect(lambda checked=False, n=number: self.load_requested.emit(n))
            buttons.addWidget(save)
            buttons.addWidget(load)
            box.addLayout(buttons)
            self.grid.addWidget(card, (number - 1) // 3, (number - 1) % 3)
