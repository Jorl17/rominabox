"""Frozen game entry point. Authoring commands and the builder are not part of game exports."""

import argparse
from pathlib import Path

from rominabox.ui.app import run

parser = argparse.ArgumentParser(prog="ROM-in-a-Box Player")
parser.add_argument("--smoke-test", type=Path, help=argparse.SUPPRESS)
args = parser.parse_args()
raise SystemExit(run(smoke_output=args.smoke_test, player_only=True))
