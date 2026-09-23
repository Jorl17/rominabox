"""Check that we report a missing test file as a visible skip, not as a pass or a failure.

    python3 scripts/test_fetch_content.py
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fetch_test_content  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
FAILURES: list[str] = []


def check(condition: bool, message: str) -> None:
    if condition:
        print(f"  ok   {message}")
    else:
        print(f"  FAIL {message}")
        FAILURES.append(message)


def a_missing_file_with_no_download_is_skipped() -> None:
    entry = {
        "name": "sample-disc",
        "path": "scripts/fixtures/sample-disc-missing.bin",
        "sourceUrl": "",
        "sha256": "",
    }
    line = fetch_test_content.skip_for(entry, ROOT)
    expected = "skipped: sample-disc not available (no download is recorded)"
    if line != expected:
        raise SystemExit(f"a missing fixture was not reported as skipped: {line}")
    print(line)


def a_failed_download_is_skipped_without_the_url() -> None:
    entry = {
        "name": "sample-disc",
        "path": "scripts/fixtures/sample-disc-missing.bin",
        "sourceUrl": "https://example.invalid/sample-disc.bin",
        "sha256": "abc",
    }

    def offline(url: str, destination: Path) -> None:
        raise OSError("offline")

    line = fetch_test_content.skip_for(entry, ROOT, fetch=offline)
    check(line == "skipped: sample-disc not available (download failed)", line or "no line")
    check(line is not None and "example.invalid" not in line, "skip line omits the download url")
    if line:
        print(line)


def the_generated_cartridge_is_ready() -> None:
    path, reason = fetch_test_content.locate("test-game")
    check(reason is None and path == ROOT / "scripts/fixtures/test-game.gbc", f"test-game: {reason}")


def an_unlisted_name_is_skipped() -> None:
    path, reason = fetch_test_content.locate("240p-dreamcast")
    check(path is None and reason is not None and "not listed" in reason, f"240p-dreamcast: {reason}")
    print(f"skipped: 240p-dreamcast not available ({reason})")


def main() -> int:
    a_missing_file_with_no_download_is_skipped()
    a_failed_download_is_skipped_without_the_url()
    the_generated_cartridge_is_ready()
    an_unlisted_name_is_skipped()
    if FAILURES:
        print(f"\n{len(FAILURES)} check(s) failed")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
