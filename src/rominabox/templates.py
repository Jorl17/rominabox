"""Select prebuilt player templates without copying the authoring application into a game."""

import json
import sys
from pathlib import Path

from .domain import AppError
from .paths import resources


def default_template(theme: str) -> Path:
    """Locate a player-only template supplied by the builder or the development build."""
    base = resources() if getattr(sys, "frozen", False) else Path(__file__).resolve().parents[2] / "dist"
    return base / "player-templates" / f"{theme}.app"


def validate_template(path: Path, theme: str) -> None:
    """Reject missing, authoring, and differently themed templates before starting an export."""
    try:
        marker = json.loads((path / "Contents/Resources/player-template.json").read_text())
        if marker != {"schema_version": 1, "kind": "player", "theme": theme}:
            raise ValueError("Incorrect player template")
        if not (path / "Contents/MacOS/ROM-in-a-Box").is_file():
            raise ValueError("Missing player executable")
    except (OSError, ValueError, TypeError) as exc:
        raise AppError("template_invalid", f"Choose a built player template for theme {theme}.") from exc
