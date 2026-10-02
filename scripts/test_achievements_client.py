"""Test the managed account boundary with the real rcheevos client and
evaluator, against a synthetic in-process service, with no RetroArch window.

    uv run python scripts/test_achievements_client.py

We compile scripts/achievements_runtime_client_test.c with the fork's
cheevos/rominabox*.c, the shared accounts store, rcheevos and libretro's file
layer, using the declared toolchain, and run it.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import core_source  # noqa: E402
import native_build  # noqa: E402
import test_accounts_store  # noqa: E402
import toolchain  # noqa: E402
from native_runtime.menu_harness import FILE_LAYER  # noqa: E402

RETROARCH = ROOT / "vendor/retroarch"
RCHEEVOS = RETROARCH / "deps/rcheevos"
OUTPUT = ROOT / "work/test-output"


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    toolchain.activate()
    binary = toolchain.executable(OUTPUT / "achievements-runtime-client-test")
    platform = core_source.host_target().split("-", 1)[0]
    sources = [
        ROOT / "scripts/achievements_runtime_client_test.c",
        RETROARCH / "cheevos/rominabox.c",
        RETROARCH / "cheevos/rominabox_catalog.c",
        RETROARCH / "cheevos/rominabox_storage.c",
        *native_build.accounts_sources(platform),
        *native_build.file_layer(platform),
        RCHEEVOS / "src/rc_client.c",
        RCHEEVOS / "src/rc_compat.c",
        RCHEEVOS / "src/rc_util.c",
        *sorted((RCHEEVOS / "src/rcheevos").glob("*.c")),
        *sorted((RCHEEVOS / "src/rapi").glob("*.c")),
        RETROARCH / "libretro-common/utils/md5.c",
        # libretro's file layer, which we use to check the player's storage folders.
        *FILE_LAYER,
    ]
    subprocess.run(
        [toolchain.describe()["cc"], "-std=gnu99", "-O0", "-g", "-Wno-deprecated-declarations", "-DRC_NO_THREADS",
         f"-I{RETROARCH}", f"-I{RETROARCH / 'deps'}", f"-I{RETROARCH / 'libretro-common/include'}",
         f"-I{RCHEEVOS / 'include'}", f"-I{native_build.accounts_folder()}",
         f"-I{ROOT / 'desktop/src-tauri/launcher'}", f"-I{ROOT / 'scripts'}",
         *map(str, sources), "-o", str(binary), *test_accounts_store.LIBRARIES[platform]],
        check=True,
    )
    return subprocess.run([str(binary)]).returncode


if __name__ == "__main__":
    sys.exit(main())
