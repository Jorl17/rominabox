"""Draw the menu pictures that we show in the builder in a browser.

In the app, we draw the menu preview in the builder with the renderer of the game,
in `menu_preview` in desktop/src-tauri/src/main.rs, with the same code as
`rominabox-cli preview`. In a browser there is no renderer, so in the builder
we show `desktop/public/menu-<design>-<palette>.png` instead. We draw those
pictures here with `rominabox-cli preview`, for every design and palette, so
that the pictures in the browser build, and in the walkthrough with which we
photograph it (scripts/builder_shots.py), are the same as the preview in the app.

    uv run python scripts/builder_menu_pictures.py

For the preview we read the designs and the renderer from the builder's
runtime kit, so draw the pictures again after building a kit with a change
to a design or to the menu.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import scratch  # noqa: E402
from built import cli  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
PUBLIC = ROOT / "desktop/public"
REGISTRY = ROOT / "desktop/designs.json"


def preview(design: str, palette: str, folder: Path) -> Path:
    """Draw the preview of `design` in `palette`, as in the builder, into `folder`."""
    request = json.dumps({"outputDir": str(folder), "theme": design, "palette": palette})
    ran = subprocess.run([str(cli()), "preview"], input=request, capture_output=True, text=True)
    lines = [json.loads(line) for line in ran.stdout.splitlines() if line.strip()]
    if ran.returncode != 0 or not lines or lines[-1].get("type") != "result":
        raise SystemExit(f"could not draw {design} in {palette}: {ran.stdout.strip()} {ran.stderr.strip()}")
    return Path(lines[-1]["result"]["imagePath"])


def main() -> int:
    registry = json.loads(REGISTRY.read_text(encoding="utf-8"))
    designs = [entry["id"] for entry in registry["designs"]]
    palettes = [entry["id"] for entry in registry["palettes"]]
    with scratch.scratch() as temporary:
        for design in designs:
            for palette in palettes:
                drawn = preview(design, palette, Path(temporary) / f"{design}-{palette}")
                target = PUBLIC / f"menu-{design}-{palette}.png"
                shutil.copyfile(drawn, target)
                print(f"  drew  {target.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
