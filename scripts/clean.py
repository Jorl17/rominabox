"""Remove generated builds and test caches, and keep downloaded runtimes and all player saves."""

import shutil
from pathlib import Path

root = Path(__file__).resolve().parents[1]
for relative in ("build", "dist", "work/frozen", "work/specs", ".pytest_cache", ".mypy_cache", ".ruff_cache"):
    path = root / relative
    if path.is_dir():
        shutil.rmtree(path)
        print(relative)
