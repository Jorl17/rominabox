"""Photograph the builder from its browser build, then exit.

We stage the controller drawings and then make the static files with
`vite build`. We check the console registries and run the typecheck in the
catalog and frontend tests, and doing that here again would only add a
second wait for the same compiler. In this script we serve the built files,
drive the flow in the Chrome already installed on the machine, and stop the
server. We download nothing and leave nothing open.

    uv run python scripts/builder_shots.py --out work/builder-shots
    uv run python scripts/builder_shots.py --out work/builder-shots-2x --scale 2
    uv run python scripts/builder_shots.py --out work/builder-window --size 1140x820 --scale 3
    uv run python scripts/builder_shots.py --check

We run --check in the builder tests. With it we walk as far as the shader
packaging controls and fail if they are not there, and keep no pictures.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

import programs

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "desktop" / "dist"
DRIVE = Path(__file__).resolve().parent / "builder_drive.mjs"


def build() -> int:
    desktop = ROOT / "desktop"
    for command in (
        [programs.require("npm"), "run", "scene"],
        [programs.require("npm"), "exec", "--", "vite", "build"],
    ):
        result = subprocess.run(
            command,
            cwd=desktop,
            capture_output=True,
            text=True,
            errors="replace",
        )
        if result.returncode != 0:
            sys.stdout.write(result.stdout)
            sys.stderr.write(result.stderr)
            return result.returncode
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, help="where to write the pictures")
    parser.add_argument("--check", action="store_true", help="walk the flow and exit; keep no pictures")
    parser.add_argument("--scale", type=int, default=1, help="device pixels per page pixel; 2 for high-density screens")
    parser.add_argument("--size", default="1440x900", help="the page's width and height, such as 1140x820, the builder window's own size")
    arguments = parser.parse_args()
    if arguments.check == (arguments.out is not None):
        raise SystemExit("pass either --check or --out")

    built = build()
    if built != 0:
        return built
    if not (DIST / "index.html").is_file():
        raise SystemExit(f"{DIST / 'index.html'} was not produced")

    command = ["node", str(DRIVE), "--dist", str(DIST)]
    if arguments.check:
        command.append("--check")
    else:
        arguments.out.mkdir(parents=True, exist_ok=True)
        command.extend(["--out", str(arguments.out.resolve())])
    width, height = arguments.size.split("x")
    shot = {**os.environ, "ROMINABOX_SHOT_SCALE": str(arguments.scale), "ROMINABOX_SHOT_WIDTH": width, "ROMINABOX_SHOT_HEIGHT": height}
    return subprocess.run(command, cwd=ROOT, env=shot).returncode


if __name__ == "__main__":
    sys.exit(main())
