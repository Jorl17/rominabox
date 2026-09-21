"""Built-in theme declarations shared by authoring, rendering and application freezing."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Theme:
    """Declare a shipped design and its MenuView implementation, never a user-supplied module."""

    id: str
    name: str
    module: str
    view_class: str
    assets: tuple[str, ...] = ()  # Paths relative to the rominabox package, including any shared theme assets.


DEFAULT_THEME = "8-bit"
THEMES = {theme.id: theme for theme in (Theme("8-bit", "8-bit", "rominabox.ui.themes.eight_bit", "EightBitMenu"),)}
