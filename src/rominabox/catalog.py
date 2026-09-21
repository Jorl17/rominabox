from __future__ import annotations

import re
import urllib.parse
import urllib.request
import zlib
from pathlib import Path

from .domain import SYSTEMS, Project
from .paths import data_home


def enrich(project: Project, cache: Path, artwork_cache: Path | None = None) -> tuple[Project, str]:
    """Match cartridge checksums locally and optionally cache box art, without uploading content."""
    artwork_cache = artwork_cache or data_home() / "cache/artwork"
    system = SYSTEMS[project.system]
    dat = cache / "catalogs" / f"{system.catalog}.dat"
    matched = False
    if dat.is_file():
        crc = 0
        with project.rom.open("rb") as stream:
            while chunk := stream.read(1024 * 1024):
                crc = zlib.crc32(chunk, crc)
        text = dat.read_text(errors="replace")
        for block in re.split(r"\bgame\s*\(", text)[1:]:
            if re.search(rf"\bcrc\s+{crc:08x}\b", block, re.I):
                name = re.search(r'\bname\s+"([^"]+)"', block)
                if name:
                    title = re.sub(r"\s*\([^)]*\)", "", name[1]).strip()
                    try:
                        project.title = title
                        matched = True
                    except ValueError:
                        pass
                    art = artwork_cache / f"{project.game_id}.png"
                    if not art.exists():
                        url = (
                            "https://raw.githubusercontent.com/libretro-thumbnails/"
                            + urllib.parse.quote(system.catalog, safe="")
                            + "/master/Named_Boxarts/"
                            + urllib.parse.quote(name[1], safe="")
                            + ".png"
                        )
                        try:
                            with urllib.request.urlopen(url, timeout=5) as response:
                                data = response.read(8 * 1024 * 1024 + 1)
                            if len(data) <= 8 * 1024 * 1024:
                                art.parent.mkdir(parents=True, exist_ok=True)
                                art.write_bytes(data)
                        except OSError:
                            pass
                    if art.exists():
                        project.icon = art
                    break
    return project, "Catalog match" if matched else "From game header / filename"
