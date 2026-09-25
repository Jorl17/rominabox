"""Build the shared accounts store and test it on real files.

    python3 scripts/test_accounts_store.py

We compile `desktop/src-tauri/accounts` with the launcher's `portable_fs` and
run `scripts/accounts_store_test.c` in a new folder under work/test-output, to
test the store's rules, private file modes, unsafe names, and several
processes changing one folder at once. We also compile the store for Windows
with zig, which shows only that the Windows code compiles.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ACCOUNTS = ROOT / "desktop/src-tauri/accounts"
PORTABLE = ROOT / "desktop/src-tauri/launcher/portable_fs.c"
OUTPUT = ROOT / "work/test-output"
SOURCES = [ACCOUNTS / "accounts.c", ACCOUNTS / "sealed.c", PORTABLE]
WARNINGS = ["-Wall", "-Wextra", "-Werror"]


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    binary = OUTPUT / "accounts-store-test"
    subprocess.run(
        ["clang", "-std=gnu99", "-O1", "-g", *WARNINGS, f"-I{ACCOUNTS}",
         str(ROOT / "scripts/accounts_store_test.c"), *map(str, SOURCES), "-o", str(binary)],
        check=True,
    )
    subprocess.run([str(binary), str(OUTPUT)], check=True)
    zig = shutil.which("zig")
    if not zig:
        print("accounts store: Windows build not checked, zig is not installed")
        return 0
    for source in SOURCES:
        subprocess.run(
            [zig, "cc", "-target", "x86_64-windows-gnu", "-std=c99", *WARNINGS,
             "-c", str(source), "-o", str(OUTPUT / f"{source.stem}-windows.o")],
            check=True,
        )
    print("accounts store: builds for Windows")
    return 0


if __name__ == "__main__":
    sys.exit(main())
