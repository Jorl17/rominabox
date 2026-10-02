"""Stage the test files listed in scripts/fixtures/test-content.json.

We list there what a test scope requires that this repository does not make.
When a file is missing and we cannot fetch it, because no download is listed,
the network is down, the URL is gone, or the bytes differ from those in the
manifest, we skip the tests that require it and print the reason. A missing
disc does not mean a broken exporter, and a silent skip would hide the test.

    uv run python scripts/fetch_test_content.py --scope quit
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "scripts/fixtures/test-content.json"


def skip_line(name: str, reason: str) -> str:
    return f"skipped: {name} not available ({reason})"


def load(path: Path | None = None) -> list[dict]:
    document = json.loads((path or MANIFEST).read_text())
    entries = document.get("entries")
    if not isinstance(entries, list):
        raise SystemExit(f"{path or MANIFEST} has no entries list")
    return entries


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    digest.update(path.read_bytes())
    return digest.hexdigest()


def _download(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(destination.name + ".partial")
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            temporary.write_bytes(response.read())
        temporary.replace(destination)
    finally:
        if temporary.is_file() and temporary != destination:
            temporary.unlink()


def skip_for(entry: dict, root: Path, fetch=_download) -> str | None:
    """None when the file is ready, otherwise the line to print for the scope."""
    name = str(entry.get("name") or "unnamed")
    relative = entry.get("path")
    if not isinstance(relative, str) or not relative:
        return skip_line(name, "the manifest names no path")
    destination = root / relative
    expected = str(entry.get("sha256") or "")
    if destination.is_file():
        if not expected or _sha256(destination) == expected:
            return None
        reason = "content changed"
    else:
        reason = ""
    url = str(entry.get("sourceUrl") or "")
    if not url:
        return skip_line(name, reason or "no download is recorded")
    try:
        fetch(url, destination)
    except Exception:
        # We leave out the exception text, because a URL in it can contain a key.
        return skip_line(name, "download failed")
    if not destination.is_file():
        return skip_line(name, "download failed")
    if expected and _sha256(destination) != expected:
        return skip_line(name, "content changed")
    return None


def locate(name: str, manifest: Path | None = None) -> tuple[Path | None, str | None]:
    """(path, None) when the file is ready, else (None, why it was skipped)."""
    root = ROOT
    for entry in load(manifest):
        if entry.get("name") != name:
            continue
        line = skip_for(entry, root)
        if line is None:
            return root / str(entry["path"]), None
        reason = line.split("(", 1)[-1].rstrip(")")
        return None, reason
    return None, f"not listed in {MANIFEST.relative_to(ROOT)}"


def skips_for_scope(scope: str) -> list[str]:
    lines: list[str] = []
    for entry in load():
        scopes = entry.get("scopes") or []
        if scope not in scopes:
            continue
        line = skip_for(entry, ROOT)
        if line:
            lines.append(line)
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scope", help="stage every entry that names this scope")
    parser.add_argument("--name", help="stage one entry")
    arguments = parser.parse_args()
    if arguments.name:
        path, reason = locate(arguments.name)
        if path is None:
            print(skip_line(arguments.name, reason or "not available"))
        return 0
    if arguments.scope:
        for line in skips_for_scope(arguments.scope):
            print(line)
        return 0
    parser.error("name a --scope or a --name")
    return 2


if __name__ == "__main__":
    sys.exit(main())
