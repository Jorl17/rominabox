"""Run formatting, lint, type, and offscreen workflow checks without opening desktop windows."""

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--runtime", action="store_true", help="Also run headless checks with the prepared native core")
args = parser.parse_args()
environment = dict(os.environ, QT_QPA_PLATFORM="offscreen")
if args.runtime:
    environment["ROMINABOX_RUNTIME_TEST"] = "1"
else:
    environment.pop("ROMINABOX_RUNTIME_TEST", None)
measurements = []
for arguments in (
    ["ruff", "format", "--check", "src", "scripts", "tests"],
    ["ruff", "check", "src", "scripts", "tests"],
    ["mypy"],
    ["pytest", "-q", "--durations=5", "--durations-min=0.1"],
):
    started = time.perf_counter()
    result = subprocess.run([sys.executable, "-m", *arguments], cwd=ROOT, env=environment)
    elapsed = time.perf_counter() - started
    measurements.append({"command": arguments, "seconds": round(elapsed, 3), "exit_code": result.returncode})
    print(f"{' '.join(arguments)}: {elapsed:.2f}s", flush=True)
    output = ROOT / "work/check-timings.json"
    output.parent.mkdir(exist_ok=True)
    output.write_text(json.dumps(measurements, indent=2))
    if result.returncode:
        raise SystemExit(result.returncode)
