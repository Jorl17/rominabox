"""Check that RetroArch would apply every pad profile that we ship.

The hid directory is the declaration. It contains every ``hid/*.cfg`` in the
pinned autoconfig archive, because we scan only that folder in the exported
player. A profile that is declared and not staged is the same failure as a
console that lists a controller defined in no package. Removing the file for a
pad is enough. The match score is then zero, and the log has "not configured".

    python3 scripts/test_joypad_autoconfig.py

We open no device and launch no player, and we do not check the button
numbers against a physical pad.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import joypad_autoconfig as joypad  # noqa: E402
import prepare_runtime  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
FAILURES: list[str] = []

# The two log lines from one launch for this pad. The debug line is in
# hexadecimal and the info line in decimal, and both are the same device.
OBSERVED_DEBUG = (
    '[Autoconf] Config files scanned: driver "hid", '
    'name "DualSense Wireless Controller" (054c/0ce6), phys "", affinity 0.'
)
OBSERVED_INFO = "[Autoconf] DualSense Wireless Controller (1356/3302) not configured."


def check(condition: bool, message: str) -> None:
    if condition:
        print(f"  ok   {message}")
    else:
        print(f"  FAIL {message}")
        FAILURES.append(message)


def pinned_hid_names() -> set[str]:
    revision = prepare_runtime.JOYPAD_AUTOCONFIG_REVISION
    archive = (
        ROOT
        / "desktop/src-tauri/resources/runtime/sources"
        / f"retroarch-joypad-autoconfig-{revision}.tar.gz"
    )
    names: set[str] = set()
    with tarfile.open(archive) as package:
        for member in package.getmembers():
            filename = prepare_runtime._hid_profile_filename(member.name)
            if filename:
                names.add(filename)
    return names


def observed_device() -> joypad.Device:
    """Return the pad from the log, parsed from the log text and not typed again."""
    debug_name = OBSERVED_DEBUG.split('name "', 1)[1].split('"', 1)[0]
    hex_ids = OBSERVED_DEBUG.split("(", 1)[1].split(")", 1)[0]
    vendor_hex, product_hex = hex_ids.split("/")
    info_name = OBSERVED_INFO.split("] ", 1)[1].rsplit(" (", 1)[0]
    decimal = OBSERVED_INFO.rsplit("(", 1)[1].split(")", 1)[0]
    vendor_dec, product_dec = decimal.split("/")
    check(debug_name == info_name, "the two log lines name the same pad")
    check(int(vendor_hex, 16) == int(vendor_dec), "vendor hex and decimal are one id")
    check(int(product_hex, 16) == int(product_dec), "product hex and decimal are one id")
    return joypad.Device(name=info_name, vendor=int(vendor_dec), product=int(product_dec))


def the_staged_set_is_the_pinned_hid_directory() -> list[joypad.Profile]:
    declared = pinned_hid_names()
    staged = {path.name for path in joypad.STAGED_HID.glob("*.cfg")}
    missing = sorted(declared - staged)
    extra = sorted(staged - declared)
    check(not missing, f"declared hid profiles are staged, missing {missing}")
    check(not extra, f"staged profiles are all declared, extra {extra}")
    check(len(staged) == len(declared) and len(declared) > 1, f"{len(staged)} profiles, not a single special case")
    return joypad.load_hid_profiles()


def the_observed_pad_matches_by_id_even_though_the_name_differs(profiles: list[joypad.Profile]) -> None:
    device = observed_device()
    winners = joypad.score_all(device, profiles)
    check(len(winners) == 1, f"exactly one profile matches the logged pad, got {len(winners)}")
    if len(winners) != 1:
        return
    winner = winners[0]
    check(
        winner.affinity >= joypad.ID_AFFINITY,
        f"vendor and product score a match (affinity {winner.affinity})",
    )
    check(
        winner.profile.entries.get("input_device") != device.name,
        "the profile name is not the name the pad reported, so the ids are what match",
    )
    same_ids = [
        profile
        for profile in profiles
        if joypad.config_int(profile.entries, "input_vendor_id") == device.vendor
        and joypad.config_int(profile.entries, "input_product_id") == device.product
    ]
    check(len(same_ids) == 1, "no second profile claims the same vendor and product")


def removing_that_profile_scores_zero(profiles: list[joypad.Profile]) -> None:
    """Check the logged failure, when the directory had no profile for this pad."""
    device = observed_device()
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        for profile in profiles:
            shutil.copy(profile.path, directory / profile.path.name)
        for profile in profiles:
            if (
                joypad.config_int(profile.entries, "input_vendor_id") == device.vendor
                and joypad.config_int(profile.entries, "input_product_id") == device.product
            ):
                (directory / profile.path.name).unlink()
        remaining = joypad.load_hid_profiles(directory)
        check(
            joypad.score_all(device, remaining) == [],
            "without that profile the logged pad is not configured",
        )


def every_profile_matches_a_device_that_reports_its_ids(profiles: list[joypad.Profile]) -> None:
    """Check that we do not require the name when both ids are present and not zero.

    In RetroArch a zero id is not an id, so those profiles must match by the
    reported name instead.
    """
    missed: list[str] = []
    by_id = 0
    by_name = 0
    for profile in profiles:
        vendor = joypad.config_int(profile.entries, "input_vendor_id") or 0
        product = joypad.config_int(profile.entries, "input_product_id") or 0
        name = profile.entries.get("input_device", "")
        if vendor and product:
            device = joypad.Device(name="name the profile does not use", vendor=vendor, product=product)
            by_id += 1
        else:
            device = joypad.Device(name=name, vendor=0, product=0)
            by_name += 1
        winners = joypad.score_all(device, profiles)
        if not any(item.profile.path == profile.path for item in winners):
            missed.append(profile.path.name)
    check(
        not missed,
        f"every profile matches a device reporting its ids, or its name when an id is zero; missed {missed}",
    )
    check(by_id > by_name, f"{by_id} profiles match by id, {by_name} only by name")


def shared_ids_are_reported_as_a_tie(profiles: list[joypad.Profile]) -> None:
    """Report two profiles with the same vendor and product as a tie, not by directory order."""
    groups: dict[tuple[int, int], list[joypad.Profile]] = {}
    for profile in profiles:
        vendor = joypad.config_int(profile.entries, "input_vendor_id") or 0
        product = joypad.config_int(profile.entries, "input_product_id") or 0
        if vendor and product:
            groups.setdefault((vendor, product), []).append(profile)
    ties = [names for names in groups.values() if len(names) > 1]
    for group in ties:
        vendor = joypad.config_int(group[0].entries, "input_vendor_id") or 0
        product = joypad.config_int(group[0].entries, "input_product_id") or 0
        winners = joypad.score_all(
            joypad.Device("name the profile does not use", vendor, product),
            profiles,
        )
        check(
            len(winners) == len(group),
            f"{vendor}/{product} is a tie across {[item.path.name for item in group]}, got {len(winners)}",
        )


def an_unknown_pad_matches_nothing(profiles: list[joypad.Profile]) -> None:
    device = joypad.Device(name="not a controller we ship", vendor=1, product=2)
    check(joypad.score_all(device, profiles) == [], "an unknown vendor and product is not configured")


def integers_parse_the_way_retroarch_does() -> None:
    check(joypad.config_int({"id": "1356"}, "id") == 1356, "a decimal id parses")
    check(joypad.config_int({"id": "0x054C"}, "id") == 0x054C, "a 0x hex id parses to the same width")
    check(joypad.config_int({"id": "054c"}, "id") is None, "a bare hex id is not silently accepted")
    check(joypad.extract_value(' "3302" ') == "3302", "quotes around an id are not part of the number")


def recognition_lines_are_not_stripped_with_hotkeys() -> None:
    planted = """\
input_driver = "hid"
input_device = "Example"
input_device_alt1 = "Example Wireless"
input_vendor_id = "1356"
input_vendor_id_alt1 = "1356"
input_product_id = "3302"
input_phys = "usb-1"
input_b_btn = "1"
input_menu_toggle_btn = "12"
"""
    stripped, removed = prepare_runtime.strip_meta_bind_lines(planted, set())
    check(removed >= 1, "a hotkey line is still removed")
    for key in (
        "input_device_alt1",
        "input_vendor_id_alt1",
        "input_product_id",
        "input_phys",
        "input_b_btn",
    ):
        check(f"{key} = " in stripped, f"{key} survives staging")
    check("input_menu_toggle_btn" not in stripped, "the menu button does not survive staging")


def the_command_line_reports_the_logged_pad() -> None:
    device = observed_device()
    matched = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts/joypad_autoconfig.py"),
            "match",
            "--name",
            device.name,
            "--vendor",
            str(device.vendor),
            "--product",
            str(device.product),
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    check(matched.returncode == 0, f"match exits 0 for the logged pad: {matched.stderr}")
    check(matched.stdout.startswith("affinity "), f"match prints a score: {matched.stdout!r}")
    unknown = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts/joypad_autoconfig.py"),
            "match",
            "--name",
            "not a controller we ship",
            "--vendor",
            "1",
            "--product",
            "2",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    check(unknown.returncode == 1, "match exits 1 when nothing fits")
    check("not configured" in unknown.stderr, "the miss uses the same words as the player log")


def main() -> int:
    profiles = the_staged_set_is_the_pinned_hid_directory()
    the_observed_pad_matches_by_id_even_though_the_name_differs(profiles)
    removing_that_profile_scores_zero(profiles)
    every_profile_matches_a_device_that_reports_its_ids(profiles)
    shared_ids_are_reported_as_a_tie(profiles)
    an_unknown_pad_matches_nothing(profiles)
    integers_parse_the_way_retroarch_does()
    recognition_lines_are_not_stripped_with_hotkeys()
    the_command_line_reports_the_logged_pad()
    if FAILURES:
        print(f"\n{len(FAILURES)} check(s) failed")
        return 1
    print("\njoypad autoconfig checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
