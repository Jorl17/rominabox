import io
import urllib.request
import zlib

from PIL import Image

from rominabox.catalog import enrich
from rominabox.domain import Project


def test_metadata_fetch_never_mutates_bundled_runtime(tmp_path, monkeypatch):
    rom = tmp_path / "game.gbc"
    rom.write_bytes(bytes(512))
    project = Project.from_rom(rom)
    kit = tmp_path / "bundle/runtime-kit"
    catalogs = kit / "catalogs"
    catalogs.mkdir(parents=True)
    crc = zlib.crc32(rom.read_bytes())
    (catalogs / "Nintendo - Game Boy Color.dat").write_text(f'game ( name "Known Game (World)" rom ( crc {crc:08x} ) )')
    image = io.BytesIO()
    Image.new("RGB", (4, 4)).save(image, "PNG")
    before = sorted(p.relative_to(kit) for p in kit.rglob("*"))
    monkeypatch.setattr(urllib.request, "urlopen", lambda *args, **kwargs: io.BytesIO(image.getvalue()))
    result, status = enrich(project, kit, artwork_cache=tmp_path / "cache")
    assert result.title == "Known Game"
    assert status == "Catalog match"
    assert sorted(p.relative_to(kit) for p in kit.rglob("*")) == before
