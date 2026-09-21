from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import IO

from .domain import SYSTEMS, AppError, Project
from .saves import SaveStore

DEFAULT_KEYS = {
    "up": "up",
    "down": "down",
    "left": "left",
    "right": "right",
    "a": "x",
    "b": "z",
    "x": "s",
    "y": "a",
    "l": "q",
    "r": "w",
    "start": "enter",
    "select": "rshift",
}


def config_text(values: dict[str, str | bool | int]) -> str:
    """Encode a strict config map without allowing values to inject new settings."""
    lines = []
    for key, value in values.items():
        text = str(value).lower() if isinstance(value, bool) else str(value)
        if any(char in text for char in ('"', "\n", "\r", "\0")):
            raise AppError("unsafe_path", "A path contains characters RetroArch cannot safely represent.")
        lines.append(f'{key} = "{text}"')
    return "\n".join(lines) + "\n"


class RuntimeSession:
    """Run one isolated RetroArch session and validate its return-to-menu checkpoint."""

    def __init__(self, project: Project, kit: Path, home: Path) -> None:
        self.project, self.kit, self.home = project, kit, home
        self.store = SaveStore(home)
        self.process: subprocess.Popen[bytes] | None = None
        self.log: IO[bytes] | None = None
        self.before: int | None = None
        self.settings: dict[str, object] = {
            "fullscreen": project.fullscreen,
            "shader": project.shader,
            "keys": DEFAULT_KEYS,
        }
        settings_file = home / "preferences.json"
        if settings_file.exists():
            try:
                data = json.loads(settings_file.read_text())
                if isinstance(data, dict):
                    self.settings.update(data)
            except ValueError:
                pass

    def save_preferences(self) -> None:
        """Persist player preferences separately from the author's game package."""
        (self.home / "preferences.json").write_text(json.dumps(self.settings, indent=2))

    def config(self, resume: bool, headless: bool = False) -> Path:
        """Write paths and predictable controls for this session without loading personal RetroArch settings."""
        for directory in ("sram", "screenshots", "system", "config", "logs"):
            (self.home / directory).mkdir(exist_ok=True)
        values: dict[str, str | bool | int] = {
            "config_save_on_exit": False,
            "savestate_auto_save": True,
            "savestate_auto_load": resume,
            "savestate_thumbnail_enable": True,
            "savestate_file_compression": True,
            "savefile_directory": str(self.home / "sram"),
            "savestate_directory": str(self.store.states),
            "screenshot_directory": str(self.home / "screenshots"),
            "system_directory": str(self.home / "system"),
            "rgui_config_directory": str(self.home / "config"),
            "core_options_path": str(self.home / "core-options.cfg"),
            "libretro_directory": str(self.kit / "cores"),
            "libretro_info_path": str(self.kit / "info"),
            "sort_savefiles_enable": False,
            "sort_savestates_enable": False,
            "sort_savefiles_by_content_enable": False,
            "sort_savestates_by_content_enable": False,
            "content_history_size": 0,
            "history_list_enable": False,
            "content_favorites_size": 0,
            "input_remap_binds_enable": False,
            "auto_overrides_enable": False,
            "video_fullscreen": bool(self.settings["fullscreen"]),
            "video_window_scale": 3,
            "video_smooth": self.settings["shader"] == "smooth",
            "video_shader_enable": False,
            "menu_driver": "rgui",
            "menu_show_core_updater": False,
            "menu_show_online_updater": False,
            "input_exit_emulator": "escape",
            "quit_press_twice": False,
            "input_menu_toggle": "f1",
            "input_quit_gamepad_combo": 4,
            "input_menu_toggle_gamepad_combo": 2,
            "pause_nonactive": True,
            "video_driver": "gl",
            "audio_driver": "coreaudio" if sys.platform == "darwin" else "sdl2",
        }
        if headless:
            values.update(
                video_driver="null",
                audio_driver="null",
                audio_enable=False,
                input_driver="null",
                input_joypad_driver="null",
                menu_driver="null",
                pause_nonactive=False,
                video_gpu_screenshot=False,
                video_vsync=False,
                audio_sync=False,
            )
        keys = self.settings.get("keys", DEFAULT_KEYS)
        if isinstance(keys, dict):
            for button in DEFAULT_KEYS:
                values[f"input_player1_{button}"] = str(keys.get(button, DEFAULT_KEYS[button]))
        path = self.home / "session.cfg"
        path.write_text(config_text(values))
        return path

    def start(self, resume: bool = False, max_frames: int | None = None, headless: bool = False) -> None:
        """Launch content with an argument array. The optional frame limit is for explicit runtime checks."""
        if self.process is not None and self.process.poll() is None:
            raise AppError("already_playing", "The game is already running.")
        executable = self.kit / "RetroArch.app/Contents/MacOS/RetroArch"
        if os.name == "nt":
            executable = self.kit / "RetroArch/retroarch.exe"
        if headless:
            executable = self.kit / "headless" / ("retroarch.exe" if os.name == "nt" else "retroarch")
            if not executable.is_file():
                raise AppError("headless_missing", "Prepare the dedicated headless runtime before running this check.")
        core = (
            self.kit
            / "cores"
            / f"{SYSTEMS[self.project.system].core}_libretro{'.dll' if os.name == 'nt' else '.dylib'}"
        )
        if not executable.is_file() or not core.is_file():
            raise AppError("runtime_missing", "The game package is missing its runtime or core.")
        self.before = self.store.checkpoint.stat().st_mtime_ns if self.store.checkpoint.exists() else None
        command = [
            str(executable),
            "-c",
            str(self.config(resume, headless)),
            "-L",
            str(core),
            str(self.project.rom),
            "--verbose",
        ]
        if max_frames is not None:
            command += ["--max-frames", str(max_frames)]
        environment = {
            key: value for key, value in os.environ.items() if not key.startswith(("QT_", "DYLD_", "_PYI_", "_MEIPASS"))
        }
        self.log = (self.home / "logs/retroarch.log").open("wb")
        try:
            self.process = subprocess.Popen(command, stdout=self.log, stderr=self.log, env=environment)
        except OSError as exc:
            self.log.close()
            raise AppError("launch_failed", f"Could not start RetroArch: {exc}") from exc

    def finish(self) -> bool:
        """Confirm process completion and a newly written nonempty checkpoint before enabling saves."""
        if self.process is None or self.process.poll() is None:
            return False
        if self.log:
            self.log.close()
        checkpoint = self.store.checkpoint
        saved = (
            self.process.returncode == 0
            and checkpoint.is_file()
            and checkpoint.stat().st_size > 0
            and checkpoint.stat().st_mtime_ns != self.before
        )
        return saved
