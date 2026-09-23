"""Check that a shot player keeps the signed sandbox and one bundle namespace.

If we replace the player, sign it with a bare `codesign --sign -` and then
use `--preserve-metadata=entitlements` on the bundle, the sandbox is lost.
We then write the shot's data into ~/Library/Application Support/ROM-in-a-Box
instead of a container. A different namespace per build directory would
create a new container each time, and we must not delete those in a script.

    python3 scripts/test_shot_sign.py
"""

from __future__ import annotations

import os
import plistlib
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import menu_shots  # noqa: E402
import scratch  # noqa: E402
import player_support  # noqa: E402

FAILURES: list[str] = []
MARKER = "shotsign-marker"


def check(condition: bool, message: str) -> None:
    if condition:
        print(f"  ok   {message}")
    else:
        print(f"  FAIL {message}")
        FAILURES.append(message)


def entitlement_text(path: Path) -> str:
    signed = subprocess.run(
        ["/usr/bin/codesign", "-d", "--entitlements", "-", str(path)],
        capture_output=True,
        text=True,
    )
    return signed.stdout + signed.stderr


def the_replaced_player_keeps_the_export_sandbox() -> None:
    """Check that the player has the entitlements that we wrote in the export."""
    with scratch.scratch("rominabox-shotsign-") as made:
        directory = Path(made)
        source = directory / "main.c"
        source.write_text("int main(void){return 0;}\n")
        binary = directory / "player"
        subprocess.run(["cc", "-Oz", "-o", str(binary), str(source)], check=True)
        app = directory / "Probe.app"
        macos = app / "Contents/MacOS"
        macos.mkdir(parents=True)
        player = macos / "retroarch"
        player.write_bytes(binary.read_bytes())
        player.chmod(0o755)
        (app / "Contents/Info.plist").write_text(
            """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>retroarch</string>
<key>CFBundleIdentifier</key><string>app.rominabox.game.shotsign</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
"""
        )
        granted = directory / "granted.plist"
        granted.write_bytes(plistlib.dumps({
            "com.apple.security.app-sandbox": True,
            "com.apple.security.temporary-exception.files.home-relative-path.read-only": [
                f"/Library/Application Support/ROM-in-a-Box/Games/{MARKER}/",
            ],
        }))
        subprocess.run(
            ["/usr/bin/codesign", "--force", "--sign", "-", str(player)],
            check=True,
        )
        subprocess.run(
            ["/usr/bin/codesign", "--force", "--sign", "-", "--entitlements", str(granted), str(app)],
            check=True,
        )
        captured = directory / "entitlements.plist"
        menu_shots.capture_export_entitlements(app, captured)
        # A fresh build, copied over the signed player as in menu_shots.
        source.write_text("int main(void){return 1;}\n")
        subprocess.run(["cc", "-Oz", "-o", str(binary), str(source)], check=True)
        player.write_bytes(binary.read_bytes())
        menu_shots.resign_replaced_player(app, captured)
        signed = entitlement_text(player)
        check(
            "com.apple.security.app-sandbox" in signed and MARKER in signed,
            "the replaced player keeps the export's app-sandbox"
            if "com.apple.security.app-sandbox" in signed and MARKER in signed
            else f"the replaced player has no app-sandbox entitlement ({signed.strip() or 'codesign printed nothing'})",
        )


def shots_reuse_one_bundle_namespace() -> None:
    """Check that menu-shots-build-0 and shaderstate-build have one identity."""
    saved = os.environ.pop("ROMINABOX_GAME_BUNDLE_PREFIX", None)
    try:
        first = menu_shots.shot_bundle_prefix(Path("menu-shots-build-0"))
        second = menu_shots.shot_bundle_prefix(Path("shaderstate-build"))
        check(
            first == second == menu_shots.SHOT_BUNDLE_PREFIX,
            f"two shot builds share {menu_shots.SHOT_BUNDLE_PREFIX}"
            if first == second == menu_shots.SHOT_BUNDLE_PREFIX
            else f"shot namespaces differ ({first} vs {second})",
        )
        os.environ["ROMINABOX_GAME_BUNDLE_PREFIX"] = "app.rominabox.game.wt-probe"
        again = menu_shots.shot_bundle_prefix(Path("menu-shots-build-1"))
        same = menu_shots.shot_bundle_prefix(Path("other"))
        check(
            again == same == "app.rominabox.game.wt-probe",
            f"a published prefix is the namespace ({again} vs {same})",
        )
    finally:
        if saved is None:
            os.environ.pop("ROMINABOX_GAME_BUNDLE_PREFIX", None)
        else:
            os.environ["ROMINABOX_GAME_BUNDLE_PREFIX"] = saved


def the_suite_names_a_path_created_under_the_real_support_directory() -> None:
    added = player_support.additions(
        frozenset(["Games/already"]),
        frozenset(["Games/already", "Games/shotsign-new/saves"]),
    )
    check(
        added == ["Games/shotsign-new/saves"],
        "a created path is named"
        if added == ["Games/shotsign-new/saves"]
        else f"a created path was reported as {added}",
    )


def the_suite_names_a_file_a_run_rewrote() -> None:
    """Check that after a second run the path is the same and the file is new."""
    before = {"games/02e5ccd65c9ebc8c79a8da5d/retroarch.cfg": (1200, 100)}
    after = {"games/02e5ccd65c9ebc8c79a8da5d/retroarch.cfg": (1200, 200)}
    changed = player_support.modifications(before, after)
    check(
        changed == ["games/02e5ccd65c9ebc8c79a8da5d/retroarch.cfg"],
        "a rewritten file is named"
        if changed == ["games/02e5ccd65c9ebc8c79a8da5d/retroarch.cfg"]
        else f"a rewritten file was reported as {changed}",
    )


def main() -> int:
    for test in (
        the_replaced_player_keeps_the_export_sandbox,
        shots_reuse_one_bundle_namespace,
        the_suite_names_a_path_created_under_the_real_support_directory,
        the_suite_names_a_file_a_run_rewrote,
    ):
        print(test.__name__)
        test()
    if FAILURES:
        print(f"\n{len(FAILURES)} check(s) failed")
        return 1
    print("\nshot signing held")
    return 0


if __name__ == "__main__":
    sys.exit(main())
