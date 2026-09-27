"""Run the typecheck, unit tests and format check at the same time.

We write the layout JSON for the typecheck with `npm run scene` first. The
three commands after it do not share state, so we run them in parallel
instead of waiting for each in turn in the frontend tests.
"""

from __future__ import annotations

import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import programs

DESKTOP = Path(__file__).resolve().parent.parent / "desktop"

STEPS = [
    ("tsc", [programs.require("npm"), "exec", "--", "tsc", "--noEmit"]),
    ("vitest", [programs.require("npm"), "exec", "--", "vitest", "run"]),
    ("prettier", [programs.require("npm"), "exec", "--", "prettier", "--check", "."]),
]


def run(name: str, command: list[str]) -> tuple[str, int, str]:
    result = subprocess.run(
        command, cwd=DESKTOP, capture_output=True, text=True, errors="replace"
    )
    text = result.stdout + result.stderr
    return name, result.returncode, text


def main() -> int:
    failed: list[str] = []
    with ThreadPoolExecutor(max_workers=len(STEPS)) as pool:
        finished = list(pool.map(lambda step: run(*step), STEPS))
    for name, code, text in finished:
        print(f"\n--- {name} ---")
        if text:
            print(text, end="" if text.endswith("\n") else "\n")
        if code != 0:
            failed.append(name)
    if failed:
        print(f"\nfrontend check failed: {', '.join(failed)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
