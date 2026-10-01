"""Build the shared accounts store and test it on real files.

    python3 scripts/test_accounts_store.py

We compile `desktop/src-tauri/accounts` with the launcher's `portable_fs` and
run `scripts/accounts_store_test.c` in a new folder under work/test-output, to
test the store's rules, how we keep an account private (file modes on macOS
and Linux, a sealed token on Windows), unsafe names, the launcher's folder,
and several processes changing one folder at once. We run the test on the
machine where we build it. On a Mac with zig we also compile the store for
Windows, which shows only that the Windows code compiles.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import core_source  # noqa: E402
import native_build  # noqa: E402
import toolchain  # noqa: E402

ACCOUNTS = native_build.accounts_folder()
# The player's root, which contains rominabox_launch.h, as in the player build.
PLAYER = ROOT / "vendor/retroarch"
OUTPUT = ROOT / "work/test-output"
WARNINGS = ["-Wall", "-Wextra", "-Werror"]
# What we link into the store on each platform. In windows/sealed.c we seal
# tokens with DPAPI, which is in crypt32 on Windows.
LIBRARIES = {"macos": [], "windows": ["-lcrypt32"]}


def sources(platform: str) -> list[Path]:
    """Return the store for `platform`, with the launcher's file layer and
    folder code that it depends on."""
    return [*native_build.accounts_sources(platform), *native_build.file_layer(platform),
            native_build.LAUNCHER / "accounts_folder.c"]


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    toolchain.activate()
    binary = toolchain.executable(OUTPUT / "accounts-store-test")
    platform = core_source.host_target().split("-", 1)[0]
    subprocess.run(
        [toolchain.describe()["cc"], "-std=gnu99", "-O1", "-g", *WARNINGS, f"-I{ACCOUNTS}", f"-I{PLAYER}",
         str(ROOT / "scripts/accounts_store_test.c"), *map(str, sources(platform)), "-o", str(binary),
         *LIBRARIES[platform]],
        check=True,
    )
    subprocess.run([str(binary), str(OUTPUT)], check=True)
    zig = shutil.which("zig")
    if not zig:
        print("accounts store: Windows build not checked, zig is not installed")
        return 0
    for source in sources("windows"):
        subprocess.run(
            [zig, "cc", "-target", "x86_64-windows-gnu", "-std=c99", *WARNINGS, f"-I{PLAYER}",
             "-c", str(source), "-o", str(OUTPUT / f"{source.parent.name}-{source.stem}-windows.o")],
            check=True,
        )
    print("accounts store: builds for Windows")
    return 0


if __name__ == "__main__":
    sys.exit(main())
