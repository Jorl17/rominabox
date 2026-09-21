"""Presentation state for all menu themes, without Qt or the emulator lifecycle."""

from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum
from pathlib import Path

from .domain import VERSION, AppError
from .saves import SaveStore

MENU_HINT = "Esc · Game menu     F1 · RetroArch options"


class SlotStatus(StrEnum):
    EMPTY = "empty"
    SAVED = "saved"
    DAMAGED = "damaged"


@dataclass(frozen=True)
class SlotState:
    number: int
    status: SlotStatus = SlotStatus.EMPTY
    preview: Path | None = None
    saved_at: datetime | None = None


@dataclass(frozen=True)
class MenuAppearance:
    accent: str
    background: Path | None = None


@dataclass(frozen=True)
class MenuState:
    """A read-only snapshot. We render it in a theme and emit intents, without save storage."""

    title: str
    can_resume: bool = False
    slots: tuple[SlotState, ...] = tuple(SlotState(n) for n in range(1, 7))
    status: str = MENU_HINT
    version: str = VERSION


def read_menu_state(title: str, store: SaveStore, can_resume: bool, status: str = MENU_HINT) -> MenuState:
    """Read each slot independently so one damaged save does not hide the other slots."""
    slots = []
    for number in range(1, 7):
        try:
            metadata = store.slot(number)
            slot = (
                SlotState(
                    number,
                    SlotStatus.SAVED,
                    store.preview(number),
                    datetime.fromtimestamp(float(str(metadata["saved_at"]))),
                )
                if metadata
                else SlotState(number)
            )
        except (AppError, ValueError, KeyError, TypeError, OverflowError, OSError):
            slot = SlotState(number, SlotStatus.DAMAGED)
        slots.append(slot)
    return MenuState(title, can_resume, tuple(slots), status)
