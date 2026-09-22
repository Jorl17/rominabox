"""Which plugged-in pads we can recognise in an export.

In exports we force the hid joypad driver. We then look only in
``joypad_autoconfig_dir`` and ``joypad_autoconfig_dir/<driver>``, one level
deep, and score each ``*.cfg`` as ``input_autoconfigure_get_config_file_affinity``
does in ``vendor/retroarch/tasks/task_autodetect.c``. This script follows
that function, so we can check a profile without a pad and without launching
a game. It is not linked into the player, so it can fall behind when that
function changes. We fail the test when a profile we ship would no longer
pass the rules as they stand.

    python3 scripts/joypad_autoconfig.py list
    python3 scripts/joypad_autoconfig.py match --name "DualSense Wireless Controller" --vendor 1356 --product 3302

``match`` exits 0 when some profile we ship scores above zero, and 1 when none
does. Vendor and product are the integers that ``config_get_int`` would read
(``strtol`` base 0), which is the decimal form in profiles and in the "not
configured" log line.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

# Bands from the comment on input_autoconfigure_get_config_file_affinity.
NAME_AFFINITY = 20
ID_AFFINITY = 30
PHYS_AFFINITY = 10
ALTERNATIVES = 10

ROOT = Path(__file__).resolve().parent.parent
STAGED_HID = ROOT / "desktop/src-tauri/resources/runtime/autoconfig/hid"
# The string passed to input_autoconfigure_connect by the macOS HID driver.
# The driver's own ident is "iohidmanager", and the scan directory has this name.
EXPORT_JOYPAD_DRIVER = "hid"


@dataclass(frozen=True)
class Profile:
    path: Path
    entries: dict[str, str]


@dataclass(frozen=True)
class Device:
    name: str
    vendor: int
    product: int
    phys: str = ""


@dataclass(frozen=True)
class Scored:
    profile: Profile
    affinity: int


def extract_value(raw: str) -> str:
    """The value ``config_file_extract_value`` would keep from one assignment."""
    value = raw.strip()
    if value.startswith('"'):
        end = value.find('"', 1)
        if end == -1:
            return ""
        return value[1:end]
    return value.split()[0] if value else ""


def parse_profile(path: Path) -> Profile:
    entries: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        body = line.strip()
        if not body or body.startswith("#") or "=" not in line:
            continue
        key, raw = line.split("=", 1)
        entries[key.strip()] = extract_value(raw)
    return Profile(path, entries)


def config_int(entries: dict[str, str], key: str) -> int | None:
    """``config_get_int``: ``strtol`` base 0, and the whole value must be consumed.

    We then store the result as ``uint16_t``, truncated to 16 bits. A leading
    zero means octal, so a hex id written as ``054c`` does not parse, because
    with ``strtol`` parsing stops at ``c`` and we reject the value.
    """
    value = entries.get(key)
    if value is None or value == "":
        return None
    try:
        if value[:2] in ("0x", "0X"):
            number = int(value, 16)
        elif len(value) > 1 and value[0] == "0" and value[1].isdigit():
            number = int(value, 8)
        else:
            number = int(value, 10)
    except ValueError:
        return None
    if number < -0x80000000 or number > 0x7FFFFFFF:
        return None
    return number & 0xFFFF


def affinity(device: Device, profile: Profile) -> int:
    """Highest score across the main entry and nine ``_altN`` alternatives.

    Zero means we will not apply the profile. We consult a physical-port line
    only after a name or id match, and then add or subtract ten for it. We add
    the alternative index so a later alternative can outrank an earlier one,
    but the index alone does not make a match.
    """
    best = 0
    for index in range(ALTERNATIVES):
        suffix = "" if index == 0 else f"_alt{index}"
        score = 0
        vendor = config_int(profile.entries, f"input_vendor_id{suffix}") or 0
        product = config_int(profile.entries, f"input_product_id{suffix}") or 0
        if (
            device.vendor == vendor
            and device.product == product
            and vendor != 0
            and product != 0
        ):
            score += ID_AFFINITY
        name = profile.entries.get(f"input_device{suffix}", "")
        if name and name == device.name:
            score += NAME_AFFINITY
        phys = profile.entries.get(f"input_phys{suffix}", "")
        if score >= NAME_AFFINITY and phys:
            if phys in device.phys:
                score += PHYS_AFFINITY
            else:
                score -= PHYS_AFFINITY
        if score > 0:
            score += index
        if score > best:
            best = score
    return best


def load_hid_profiles(directory: Path | None = None) -> list[Profile]:
    root = directory or STAGED_HID
    if not root.is_dir():
        raise SystemExit(f"no hid profiles at {root}")
    return [parse_profile(path) for path in sorted(root.glob("*.cfg"))]


def score_all(device: Device, profiles: list[Profile]) -> list[Scored]:
    """Every profile that matches, best score first. A tie is the whole list.

    In RetroArch we keep the first file with a strictly higher score, and stop
    scanning above 60. Directory order is not stable, so we report a tie as a
    tie instead of choosing a winner that could differ in the player.
    """
    scored = [Scored(profile, affinity(device, profile)) for profile in profiles]
    best = max((item.affinity for item in scored), default=0)
    if best <= 0:
        return []
    return [item for item in scored if item.affinity == best]


def list_profiles(profiles: list[Profile]) -> None:
    for profile in profiles:
        vendor = config_int(profile.entries, "input_vendor_id")
        product = config_int(profile.entries, "input_product_id")
        name = profile.entries.get("input_device", "")
        vendor_text = "" if vendor is None else str(vendor)
        product_text = "" if product is None else str(product)
        print(f"{vendor_text:>6} {product_text:>6}  {name}  [{profile.path.name}]")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--directory", type=Path, default=STAGED_HID, help="hid profile directory")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("list", help="every staged hid profile")

    match = sub.add_parser("match", help="score one reported device against the staged profiles")
    match.add_argument("--name", required=True)
    match.add_argument("--vendor", required=True, help="decimal, or 0x-prefixed hex, as config_get_int reads it")
    match.add_argument("--product", required=True)
    match.add_argument("--phys", default="")

    arguments = parser.parse_args(argv)
    profiles = load_hid_profiles(arguments.directory)
    if arguments.command == "list":
        list_profiles(profiles)
        print(f"{len(profiles)} hid profiles")
        return 0

    device = Device(
        name=arguments.name,
        vendor=config_int({"id": arguments.vendor}, "id") or 0,
        product=config_int({"id": arguments.product}, "id") or 0,
        phys=arguments.phys,
    )
    winners = score_all(device, profiles)
    if not winners:
        print(
            f"not configured: {device.name} ({device.vendor}/{device.product})",
            file=sys.stderr,
        )
        return 1
    if len(winners) > 1:
        print(
            f"tie at affinity {winners[0].affinity}: "
            + ", ".join(item.profile.path.name for item in winners),
            file=sys.stderr,
        )
        return 1
    winner = winners[0]
    print(f"affinity {winner.affinity}  {winner.profile.path.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
