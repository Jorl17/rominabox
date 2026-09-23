"""A temporary directory that we remove when the block ends, also on failure.

With `mkdtemp`, a `rominabox-*` directory stays in `$TMPDIR` whenever the line
that removes it does not run. We use `TemporaryDirectory` for the removal,
and tests get their directories here instead of from `mkdtemp`.
"""

from __future__ import annotations

import os
import tempfile


def scratch(prefix: str = "rominabox-") -> tempfile.TemporaryDirectory:
    if not prefix or "/" in prefix or "\\" in prefix or ".." in prefix:
        raise ValueError(f"scratch prefix must be one path component, got {prefix!r}")
    # The same stamp as on the Rust side. We report only names that contain it.
    run = os.environ.get("ROMINABOX_SCRATCH_RUN", "direct")
    if not run or "/" in run or "\\" in run or ".." in run:
        raise ValueError(f"scratch run id must be one path component, got {run!r}")
    return tempfile.TemporaryDirectory(prefix=f"{prefix}{run}-")
