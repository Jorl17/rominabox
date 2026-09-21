"""Qt menu contract and creation from the built-in theme registry."""

from importlib import import_module

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QWidget

from ..domain import AppError
from ..menu import MenuAppearance, MenuState
from ..themes import THEMES


class MenuView(QWidget):
    """Lay out and draw a theme. Emit actions without handling storage or emulation."""

    play_requested = Signal()
    fresh_requested = Signal()
    save_requested = Signal(int)
    load_requested = Signal(int)
    options_requested = Signal()
    quit_requested = Signal()
    about_requested = Signal()

    def __init__(self, appearance: MenuAppearance, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.appearance = appearance
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground)

    def present(self, state: MenuState) -> None:
        """Refresh visible state while retaining this view's layout and interaction conventions."""
        raise NotImplementedError


def create_menu(theme_id: str, appearance: MenuAppearance, parent: QWidget | None = None) -> MenuView:
    """Instantiate only a registered built-in renderer. Project files contain IDs, not import paths."""
    try:
        theme = THEMES[theme_id]
    except KeyError:
        raise AppError("theme_unknown", f"Unknown menu theme: {theme_id}") from None
    view_type = getattr(import_module(theme.module), theme.view_class)
    if not isinstance(view_type, type) or not issubclass(view_type, MenuView):
        raise AppError("theme_invalid", f"{theme.name} does not implement the menu view contract.")
    return view_type(appearance, parent)
