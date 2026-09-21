from __future__ import annotations

import hashlib
import re
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator

from .themes import DEFAULT_THEME, THEMES

VERSION = "0.1.0"


class AppError(Exception):
    """An actionable error shared by the CLI and desktop interfaces."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


@dataclass(frozen=True)
class System:
    """Declare a console's content extensions, default core, and catalog."""

    id: str
    name: str
    extensions: tuple[str, ...]
    core: str
    catalog: str


SYSTEMS = {
    s.id: s
    for s in (
        System(
            "megadrive",
            "Mega Drive / Genesis",
            (".md", ".gen", ".smd"),
            "genesis_plus_gx",
            "Sega - Mega Drive - Genesis",
        ),
        System("gbc", "Game Boy Color", (".gbc",), "gambatte", "Nintendo - Game Boy Color"),
        System("gb", "Game Boy", (".gb",), "gambatte", "Nintendo - Game Boy"),
        System("gba", "Game Boy Advance", (".gba",), "mgba", "Nintendo - Game Boy Advance"),
        System("nes", "NES", (".nes",), "nestopia", "Nintendo - Nintendo Entertainment System"),
        System("snes", "Super Nintendo", (".sfc", ".smc"), "snes9x", "Nintendo - Super Nintendo Entertainment System"),
        System("n64", "Nintendo 64", (".z64", ".n64", ".v64"), "mupen64plus_next", "Nintendo - Nintendo 64"),
        System("gamegear", "Game Gear", (".gg",), "genesis_plus_gx", "Sega - Game Gear"),
        System("mastersystem", "Master System", (".sms",), "genesis_plus_gx", "Sega - Master System - Mark III"),
        System("ps1", "PlayStation", (".cue", ".chd", ".pbp"), "swanstation", "Sony - PlayStation"),
        System("ps2", "PlayStation 2", (".iso", ".chd"), "pcsx2", "Sony - PlayStation 2"),
        System("dreamcast", "Dreamcast", (".gdi", ".chd", ".cdi"), "flycast", "Sega - Dreamcast"),
        System("gamecube", "GameCube", (".gcm", ".rvz", ".iso"), "dolphin", "Nintendo - GameCube"),
        System("atari2600", "Atari 2600", (".a26",), "stella", "Atari - 2600"),
    )
}


def file_hash(path: Path) -> str:
    """Hash a file without loading an entire disc image into memory."""
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


class Project(BaseModel):
    """The versioned authoring contract that we use unchanged in the GUI and CLI."""

    model_config = ConfigDict(extra="forbid", validate_assignment=True)
    schema_version: Literal[1] = 1
    game_id: str = Field(pattern=r"^[a-f0-9]{32}$")
    title: str = Field(min_length=1, max_length=100)
    system: str
    rom: Path
    description: str = ""
    icon: Path | None = None
    background: Path | None = None
    theme: str = DEFAULT_THEME
    show_menu: bool = True
    fullscreen: bool = False
    autosave: bool = True
    accent: str = Field(default="#1647d8", pattern=r"^#[0-9a-fA-F]{6}$")
    shader: Literal["clean", "smooth"] = "clean"

    @field_validator("theme")
    @classmethod
    def valid_theme(cls, value: str) -> str:
        """Keep persisted theme choices aligned with the shipped theme registry."""
        if value not in THEMES:
            raise ValueError("Choose a theme listed by rominabox themes.")
        return value

    @field_validator("system")
    @classmethod
    def valid_system(cls, value: str) -> str:
        """Reject console identifiers with no declared behavior."""
        if value not in SYSTEMS:
            raise ValueError("Choose a supported system.")
        return value

    @field_validator("title")
    @classmethod
    def valid_title(cls, value: str) -> str:
        """Keep titles usable as filenames on all target platforms."""
        value = value.strip()
        if not value or value in {".", ".."} or re.search(r'[<>:"/\\|?*\x00-\x1f]', value):
            raise ValueError("Use a title without filename separators or reserved characters.")
        if value.endswith(".") or re.fullmatch(r"(?i)(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?", value):
            raise ValueError("Choose a title that is valid on Windows.")
        return value

    @classmethod
    def from_rom(cls, path: Path, system: str | None = None) -> Project:
        """Inspect a local game and create a stable identity independent of its title."""
        path = path.expanduser().resolve()
        if not path.is_file():
            raise AppError("rom_missing", "Choose an existing game file.")
        with path.open("rb") as stream:
            header = stream.read(512)
        if system is None:
            candidates = [s.id for s in SYSTEMS.values() if path.suffix.lower() in s.extensions]
            if header[0x100:0x104] == b"SEGA":
                candidates = ["megadrive"]
            if len(candidates) != 1:
                raise AppError("system_required", "Choose the system for this game.")
            system = candidates[0]
        title = path.stem
        if system == "megadrive" and header[0x100:0x104] == b"SEGA":
            title = header[0x150:0x180].decode("ascii", errors="ignore").strip() or title
        elif system in {"gb", "gbc"} and len(header) > 0x143:
            title = header[0x134:0x143].split(b"\0")[0].decode("ascii", errors="ignore").strip() or title
        title = re.sub(r'[<>:"/\\|?*\x00-\x1f]', " ", title)
        title = " ".join(title.split()).strip(" .") or "Untitled game"
        digest = file_hash(path)
        return cls(
            game_id=uuid.uuid5(uuid.NAMESPACE_URL, f"rominabox:{system}:{digest}").hex,
            title=title[:100],
            system=system,
            rom=path,
        )

    def write(self, path: Path) -> None:
        """Persist the authoring project as readable JSON."""
        path.write_text(self.model_dump_json(indent=2) + "\n")

    @classmethod
    def read(cls, path: Path) -> Project:
        """Resolve project-relative inputs against the project file's directory."""
        project = cls.model_validate_json(path.read_text())
        for name in ("rom", "icon", "background"):
            value = getattr(project, name)
            if value is not None and not value.is_absolute():
                setattr(project, name, (path.parent / value).resolve())
        return project
