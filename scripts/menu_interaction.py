"""Record what happens in the menu on a click, and fail when that changes.

This covers hover, press and focus behaviour. In the bridge we protect that
behaviour with checks such as HoverListener's `if (hovered_action == action)`
against a Mouseout that arrives after the next Mouseover.

We record the menu's behaviour so that we can compare a change to the element
model with it. We drive the actual `menu.rml` through RmlUi with synthetic
input and no renderer (no window, no OpenGL, no RetroArch), and write the
resulting RmlUi state as JSON Lines.

    python3 scripts/menu_interaction.py --record   # write the baseline
    python3 scripts/menu_interaction.py --check    # fail on any difference

We record hit testing and event dispatch: which element is under the pointer,
and the classes on the elements that we style in a design. The defects are in
that layer. This does not show that the menu appears correctly, and it does
not run the C++ bridge, which we do not load here.
"""

from __future__ import annotations

import argparse
import difflib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
# We link the probe with the same archive as the player build.
import rmlui_paths  # noqa: E402
from rmlui_paths import HEADER_DIRS, LIBRARY  # noqa: E402
from built import cli  # noqa: E402

PROBE = ROOT / "work/probe/rml_probe"
PROBE_SOURCE = ROOT / "scripts/native_runtime/rml_probe.cpp"
DESIGN = ROOT / "integrations/designs/native"
ASSETS = ROOT / "work/probe/menu-assets"
DOCUMENT = ASSETS / "menu.rml"
# The expected result after staging. It differs from the raw-document fixture
# only in the Controls option-entry class, and the rest is the same.
BASELINE = ROOT / "scripts/fixtures/menu-interaction.jsonl"

# One scenario with every interaction in the menu. Keep the watched elements
# stable, because a baseline is only useful while we can compare with it.
SCENARIO = [
    "watch:resume", "watch:load", "watch:controls",
    "watch:slot-1", "watch:slot-2",
    "watch:pause-panel", "watch:controls-panel",
    # Resting state, before anything is touched.
    "move:0,0",
    # A slot, and the nested label inside it. The pointer is on the child, as
    # it will be on the options of a dropdown.
    "move:100,100",
    "move:480,300",
    # Press and release separately, so an :active style that remains after the
    # release would show up here rather than in a bug report.
    "down:100,100",
    "up:100,100",
    # Straight from one slot to another without passing over the gap. This is
    # the order of events for which we check Mouseout in the bridge.
    "move:100,100",
    "move:480,300",
    # Off every interactive element again.
    "move:0,0",
    # Keyboard input, which changes focus and not hover.
    "key:down",
    "key:down",
    "key:up",
    "key:return",
    "key:escape",
]


def build() -> None:
    """Rebuild when the probe, build recipe or linked RmlUi archive changes."""
    if not LIBRARY.is_file():
        raise SystemExit(
            f"RmlUi is not built at {LIBRARY}.\n"
            "python3 scripts/prepare_rmlui.py"
        )
    inputs = (PROBE_SOURCE, Path(__file__), ROOT / "scripts/rmlui_paths.py", LIBRARY)
    if PROBE.exists() and PROBE.stat().st_mtime >= max(path.stat().st_mtime for path in inputs):
        return
    PROBE.parent.mkdir(parents=True, exist_ok=True)
    flags = rmlui_paths.freetype("--cflags", "--libs")
    subprocess.run(
        ["c++", "-std=c++17", "-O1",
         *[f"-I{path}" for path in HEADER_DIRS],
         "-o", str(PROBE), str(PROBE_SOURCE), str(LIBRARY), *flags],
        check=True,
    )


def run() -> str:
    build()
    subprocess.run(
        [str(cli()), "stage-theme"],
        input=json.dumps({"source": str(DESIGN), "destination": str(ASSETS), "palette": "blue"}),
        capture_output=True, text=True, check=True,
    )
    steps: list[str] = []
    for entry in SCENARIO:
        steps += ["--step", entry]
    result = subprocess.run(
        [str(PROBE), "--document", str(DOCUMENT), "--size", "960x600", *steps],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise SystemExit(f"probe failed ({result.returncode}):\n{result.stderr}")
    return result.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--record", action="store_true", help="write the baseline")
    group.add_argument("--check", action="store_true", help="compare against it")
    arguments = parser.parse_args()

    observed = run()
    if arguments.record:
        BASELINE.parent.mkdir(parents=True, exist_ok=True)
        BASELINE.write_text(observed)
        print(f"recorded {len(observed.splitlines())} steps -> {BASELINE}")
        return 0

    if not BASELINE.exists():
        raise SystemExit(f"no baseline at {BASELINE}; run --record first")
    expected = BASELINE.read_text()
    if observed == expected:
        print(f"menu interaction unchanged across {len(expected.splitlines())} steps")
        return 0

    print("menu interaction CHANGED:\n", file=sys.stderr)
    for line in difflib.unified_diff(
        expected.splitlines(), observed.splitlines(),
        fromfile="baseline", tofile="observed", lineterm="",
    ):
        print(line, file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
