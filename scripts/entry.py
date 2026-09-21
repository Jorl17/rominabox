"""Frozen entry point. We open the GUI on a plain launch, and the shared engine with --cli."""

import sys
from pathlib import Path

from rominabox.cli import main
from rominabox.ui.app import run

if len(sys.argv) > 1 and sys.argv[1] == "--cli":
    raise SystemExit(main(sys.argv[2:]))
if len(sys.argv) == 3 and sys.argv[1] == "--smoke-test":
    raise SystemExit(run(smoke_output=Path(sys.argv[2])))
raise SystemExit(run())
