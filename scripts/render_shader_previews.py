"""Render each shader's preview by running the shader, and check it has not drifted.

We make the picture of a filter by applying the filter, so every preset in
the catalogue has a preview, a change to a fragment changes its preview, and
a shader that an author supplies can have a preview too.

    python3 scripts/render_shader_previews.py          # regenerate every preview
    python3 scripts/render_shader_previews.py --check  # fail if any has drifted

We take the GLSL from the exporter, so we compile the same GLSL as in an
exported game and not a copy of it. Rendering requires the Chrome already on
the machine. We download nothing, and an exported game does not include
build tools.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PREVIEWS = ROOT / "integrations/shaders/previews"
DRIVER = Path(__file__).resolve().parent / "shader_previews.mjs"

sys.path.insert(0, str(Path(__file__).resolve().parent))
from built import cli as _cli  # noqa: E402

# The same tolerances as for the controller artwork, for the same reason. We
# check that it is the same picture, not that two encoders wrote the same
# bytes. Different versions of a software rasteriser round the last bit
# differently.
PIXEL_TOLERANCE = 1.0  # percent of pixels that may differ at all
CHANNEL_TOLERANCE = 6  # how far one of those pixels may be off, per channel


def sources() -> list[dict]:
    """Return every shader that can have a preview, with the source we make it from."""
    asked = subprocess.run(
        [str(_cli()), "shader-sources"], capture_output=True, text=True, timeout=120
    )
    if asked.returncode != 0:
        detail = asked.stderr.strip() or asked.stdout.strip()
        raise SystemExit(f"could not ask the exporter for the shader sources: {detail}")
    return json.loads(asked.stdout)["result"]["shaders"]


def draw(shaders: list[dict], destination: Path) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        listing = Path(temporary) / "shaders.json"
        listing.write_text(json.dumps(shaders))
        drawn = subprocess.run(
            [
                "node",
                str(DRIVER),
                "--shaders",
                str(listing),
                "--out",
                str(destination),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=300,
        )
        sys.stdout.write(drawn.stdout)
        if drawn.returncode != 0:
            raise SystemExit(drawn.stderr.strip() or "the preview renderer failed")


def compare(recorded: Path, fresh: Path) -> str:
    """Return why the preview does not match, or nothing when it does."""
    from PIL import Image

    a = Image.open(recorded).convert("RGBA")
    b = Image.open(fresh).convert("RGBA")
    if a.size != b.size:
        return f"size {b.size} against {a.size}"
    differing = 0
    for p, q in zip(a.getdata(), b.getdata()):
        if max(abs(x - y) for x, y in zip(p, q)) > CHANNEL_TOLERANCE:
            differing += 1
    percent = 100 * differing / (a.size[0] * a.size[1])
    if percent > PIXEL_TOLERANCE:
        return f"{percent:.1f}% of pixels differ, which is more than rounding"
    return ""


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare the recorded previews against a fresh render and fail on drift",
    )
    arguments = parser.parse_args()

    shaders = sources()
    if not shaders:
        raise SystemExit("the exporter offers no shaders, so there is nothing to draw")

    if not arguments.check:
        draw(shaders, PREVIEWS)
        print(f"\nrendered {len(shaders)} shader previews -> {PREVIEWS}")
        return 0

    with tempfile.TemporaryDirectory() as temporary:
        scratch = Path(temporary)
        draw(shaders, scratch)
        drifted: list[str] = []
        for shader in shaders:
            name = f"{shader['id']}.png"
            recorded = PREVIEWS / name
            if not recorded.is_file():
                print(f"  MISSING {name}", file=sys.stderr)
                drifted.append(shader["id"])
                continue
            verdict = compare(recorded, scratch / name)
            if verdict:
                print(f"  DRIFTED {name}: {verdict}", file=sys.stderr)
                drifted.append(shader["id"])
            else:
                print(f"  ok      {name}")
        extra = sorted(
            path.name
            for path in PREVIEWS.glob("*.png")
            if path.stem not in {shader["id"] for shader in shaders}
        )
        for name in extra:
            print(f"  ORPHAN  {name}: no shader in the catalogue draws this", file=sys.stderr)
        if drifted or extra:
            print(
                f"\n{len(drifted) + len(extra)} preview(s) do not match the shaders "
                "that make them. The shader is the source. Regenerate with:\n"
                "  python3 scripts/render_shader_previews.py",
                file=sys.stderr,
            )
            return 1
        print(f"\nall {len(shaders)} previews match the shader that makes them")
        return 0


if __name__ == "__main__":
    sys.exit(main())
