import json
import os
from pathlib import Path

import pytest

from rominabox.domain import AppError, Project
from rominabox.runtime import RuntimeSession
from rominabox.saves import SaveStore


def cartridge(tmp_path: Path) -> Project:
    rom = tmp_path / "my game.gbc"
    rom.write_bytes(bytes(512))
    return Project.from_rom(rom)


def test_identity_survives_rename_and_project_roundtrip(tmp_path):
    project = cartridge(tmp_path)
    identity = project.game_id
    project.title = "A different name"
    project.write(tmp_path / "project.json")
    assert Project.read(tmp_path / "project.json").game_id == identity
    renamed = tmp_path / "renamed.gbc"
    project.rom.rename(renamed)
    assert Project.from_rom(renamed).game_id == identity


def test_ambiguous_disc_requires_system(tmp_path):
    rom = tmp_path / "game.iso"
    rom.write_bytes(b"disc")
    with pytest.raises(AppError, match="Choose the system"):
        Project.from_rom(rom)


def test_runtime_configuration_without_unix_apis(tmp_path, monkeypatch):
    project = cartridge(tmp_path)
    monkeypatch.delattr(os, "uname", raising=False)
    session = RuntimeSession(project, tmp_path / "kit", tmp_path / "user data")
    config = session.config(resume=True).read_text()
    assert 'savestate_auto_load = "true"' in config
    assert str(tmp_path / "user data/sram") in config
    assert 'config_save_on_exit = "false"' in config


def test_slots_restore_matching_state_and_preview(tmp_path):
    store = SaveStore(tmp_path)
    preview = store.checkpoint.with_name(store.checkpoint.name + ".png")
    store.checkpoint.write_bytes(b"first state")
    preview.write_bytes(b"first preview")
    store.save(1)
    store.checkpoint.write_bytes(b"second state")
    preview.write_bytes(b"second preview")
    store.save(2)
    store.restore(1)
    assert store.checkpoint.read_bytes() == b"first state"
    assert preview.read_bytes() == b"first preview"
    assert store.preview(2).read_bytes() == b"second preview"


def test_interrupted_save_preserves_existing_slot(tmp_path, monkeypatch):
    store = SaveStore(tmp_path)
    store.checkpoint.write_bytes(b"original")
    store.save(1)
    store.checkpoint.write_bytes(b"new")

    def disk_failure(*args):
        raise OSError("disk failure")

    with monkeypatch.context() as context:
        context.setattr(os, "replace", disk_failure)
        with pytest.raises(OSError):
            store.save(1)
    store.restore(1)
    assert store.checkpoint.read_bytes() == b"original"


def test_slot_reference_cannot_escape_storage(tmp_path):
    store = SaveStore(tmp_path)
    (store.slots / "1.json").write_text(json.dumps({"generation": "../elsewhere"}))
    with pytest.raises(AppError, match="damaged"):
        store.restore(1)


def test_headless_config_uses_no_display_audio_or_controller(tmp_path):
    session = RuntimeSession(cartridge(tmp_path), tmp_path / "kit", tmp_path / "data")
    config = session.config(False, headless=True).read_text()
    for driver in ("video", "audio", "input", "input_joypad", "menu"):
        assert f'{driver}_driver = "null"' in config
    assert 'pause_nonactive = "false"' in config


def test_headless_never_falls_back_to_desktop_app(tmp_path, package_inputs, monkeypatch):
    kit, _ = package_inputs
    session = RuntimeSession(cartridge(tmp_path), kit, tmp_path / "data")
    import subprocess

    def forbid_desktop(*args, **kwargs):
        pytest.fail("Headless mode attempted to launch the desktop application")

    monkeypatch.setattr(subprocess, "Popen", forbid_desktop)
    with pytest.raises(AppError, match="dedicated headless runtime"):
        session.start(headless=True, max_frames=10)


def test_stale_checkpoint_cannot_be_offered_as_a_new_save(tmp_path):
    import subprocess

    session = RuntimeSession(cartridge(tmp_path), tmp_path / "kit", tmp_path / "data")
    session.store.checkpoint.write_bytes(b"old")
    session.before = session.store.checkpoint.stat().st_mtime_ns
    session.process = subprocess.CompletedProcess([], 0)
    session.process.poll = lambda: 0
    assert session.finish() is False
    session.store.checkpoint.write_bytes(b"new")
    assert session.finish() is True
    session.process.returncode = 1
    assert session.finish() is False
