from __future__ import annotations

import json
import os
import shutil
import time
import uuid
from pathlib import Path

from .domain import AppError


class SaveStore:
    """Keep immutable state and preview pairs, and atomically publish a pointer per manual slot."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.states = root / "states"
        self.slots = root / "slots"
        self.states.mkdir(parents=True, exist_ok=True)
        self.slots.mkdir(parents=True, exist_ok=True)

    @property
    def checkpoint(self) -> Path:
        """Return RetroArch's clean-exit checkpoint for canonical game content."""
        return self.states / "game.state.auto"

    def slot(self, number: int) -> dict[str, object] | None:
        """Read validated slot metadata, or return None for an unused slot."""
        self._validate_slot(number)
        path = self.slots / f"{number}.json"
        if not path.exists():
            return None
        try:
            data = json.loads(path.read_text())
            generation = data["generation"]
            if (
                not isinstance(generation, str)
                or len(generation) != 32
                or any(c not in "0123456789abcdef" for c in generation)
            ):
                raise ValueError("Invalid slot reference")
            if not (self.slots / generation / "state").is_file():
                raise ValueError("Missing state data")
            return dict(data)
        except (ValueError, KeyError, TypeError) as exc:
            raise AppError("slot_damaged", f"Save slot {number} is damaged. Other slots are unaffected.") from exc

    def save(self, number: int) -> None:
        """Publish a copy of the last verified checkpoint without risking an existing slot."""
        self._validate_slot(number)
        if not self.checkpoint.is_file() or self.checkpoint.stat().st_size == 0:
            raise AppError("no_checkpoint", "Play the game and return to the menu before saving a slot.")
        generation = uuid.uuid4().hex
        directory = self.slots / generation
        directory.mkdir()
        shutil.copy2(self.checkpoint, directory / "state")
        preview = self.checkpoint.with_name(self.checkpoint.name + ".png")
        if preview.is_file():
            shutil.copy2(preview, directory / "preview.png")
        metadata = {"generation": generation, "saved_at": time.time(), "preview": preview.is_file()}
        temporary = self.slots / f".{number}-{generation}.tmp"
        temporary.write_text(json.dumps(metadata))
        os.replace(temporary, self.slots / f"{number}.json")

    def restore(self, number: int) -> None:
        """Atomically select a manual slot as the next resume checkpoint."""
        data = self.slot(number)
        if data is None:
            raise AppError("slot_empty", f"Slot {number} has no save yet.")
        source = self.slots / str(data["generation"])
        temporary = self.states / ".restore.tmp"
        shutil.copy2(source / "state", temporary)
        os.replace(temporary, self.checkpoint)
        preview = self.checkpoint.with_name(self.checkpoint.name + ".png")
        if (source / "preview.png").exists():
            shutil.copy2(source / "preview.png", preview)
        else:
            preview.unlink(missing_ok=True)

    def preview(self, number: int) -> Path | None:
        """Return the preview belonging to a slot's published state generation."""
        data = self.slot(number)
        if data is None:
            return None
        path = self.slots / str(data["generation"]) / "preview.png"
        return path if path.is_file() else None

    @staticmethod
    def _validate_slot(number: int) -> None:
        """Restrict manual saves to the six visible slots."""
        if not 1 <= number <= 6:
            raise AppError("invalid_slot", "Choose a save slot from 1 to 6.")
