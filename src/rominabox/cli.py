from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from pydantic import ValidationError

from . import packaging
from .catalog import enrich
from .domain import SYSTEMS, VERSION, AppError, Project
from .paths import runtime_kit
from .themes import DEFAULT_THEME, THEMES


def main(argv: list[str] | None = None) -> int:
    """Expose the same authoring contract as the desktop app with machine-readable results."""
    parser = argparse.ArgumentParser(prog="rominabox", description="Package a game as a standalone app.")
    parser.add_argument("--version", action="version", version=VERSION)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("gui", help="Open the desktop builder")
    commands.add_parser("systems", help="List configured systems and available cores")
    commands.add_parser("themes", help="List shipped menu themes")
    play = commands.add_parser("play", help="Run a project with isolated saves (headless checks supported)")
    play.add_argument("project", type=Path)
    play.add_argument("--runtime-kit", type=Path, default=runtime_kit())
    play.add_argument("--data-dir", type=Path, required=True)
    play.add_argument("--headless", action="store_true")
    play.add_argument("--frames", type=int)
    play.add_argument("--resume", action="store_true")
    inspect = commands.add_parser("inspect", help="Inspect a ROM and emit a project as JSON")
    inspect.add_argument("rom", type=Path)
    inspect.add_argument("--system", choices=SYSTEMS)
    inspect.add_argument("--theme", choices=THEMES, default=DEFAULT_THEME)
    inspect.add_argument("--lookup", action="store_true", help="Use local catalogs and download matching artwork")
    for name in ("validate", "build"):
        sub = commands.add_parser(name, help=f"{name.capitalize()} a project JSON file")
        sub.add_argument("project", type=Path)
        sub.add_argument("--runtime-kit", type=Path, default=runtime_kit())
        if name == "build":
            sub.add_argument("--output", type=Path, required=True)
            sub.add_argument("--template", type=Path)
            sub.add_argument("--progress", action="store_true", help="Emit JSON progress events on stderr")
    args = parser.parse_args(argv)
    result: object
    try:
        if args.command == "gui":
            from .ui.app import run

            return run()
        if args.command == "systems":
            available = packaging.available_systems(runtime_kit())
            result = [
                {"id": s.id, "name": s.name, "core": s.core, "available": s.id in available} for s in SYSTEMS.values()
            ]
        elif args.command == "themes":
            result = [
                {"id": theme.id, "name": theme.name, "default": theme.id == DEFAULT_THEME} for theme in THEMES.values()
            ]
        elif args.command == "inspect":
            project = Project.from_rom(args.rom, args.system)
            project.theme = args.theme
            if args.lookup:
                project, _ = enrich(project, runtime_kit())
            result = project.model_dump(mode="json")
        elif args.command == "play":
            import subprocess

            from .runtime import RuntimeSession

            if args.frames is not None and args.frames < 1:
                raise AppError("invalid_frames", "Frame count must be positive.")
            session = RuntimeSession(Project.read(args.project), args.runtime_kit, args.data_dir)
            session.start(args.resume, args.frames, args.headless)
            assert session.process is not None
            try:
                session.process.wait(timeout=60 if args.frames else None)
            except (subprocess.TimeoutExpired, KeyboardInterrupt):
                session.process.kill()
                session.process.wait()
                session.finish()
                raise AppError("run_cancelled", "The emulator run was cancelled or exceeded its time limit.") from None
            if not session.finish():
                raise AppError(
                    "checkpoint_failed", f"No new checkpoint was saved. See {session.home / 'logs/retroarch.log'}"
                )
            result = {"checkpoint": str(session.store.checkpoint)}
        elif args.command == "validate":
            project = Project.read(args.project)
            packaging.validate(project, args.runtime_kit)
            result = {"valid": True, "game_id": project.game_id}
        else:
            path = packaging.build(
                Project.read(args.project),
                args.output,
                args.runtime_kit,
                args.template,
                progress=lambda message: (
                    print(json.dumps({"progress": message}), file=sys.stderr) if args.progress else None
                ),
            )
            result = {"path": str(path)}
        print(json.dumps(result, indent=2))
        return 0
    except (AppError, ValidationError, OSError) as exc:
        print(
            json.dumps({"error": {"code": getattr(exc, "code", "invalid_input"), "message": str(exc)}}), file=sys.stderr
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
