"""Check that we never stop preparing cores and profiles over a licence text.

Licences are for attribution. When a text is missing from a source archive,
or an upstream text no longer matches the entry in licenses/, we name it in
a warning and stage what it belongs to all the same. In each case we build
the archive in a temporary folder, so nothing here uses the network.

    python scripts/test_preparation.py
"""

from __future__ import annotations

import contextlib
import io
import sys
import tarfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import prepare_runtime  # noqa: E402
import scratch  # noqa: E402

FAILURES: list[str] = []

# The only profile required in a hid folder: the pad we test the product with.
DUALSENSE_PROFILE = (
    'input_driver = "hid"\n'
    'input_device = "DualSense Wireless Controller"\n'
    'input_vendor_id = "1356"\n'
    'input_product_id = "3302"\n'
    'input_b_btn = "1"\n'
)
# The profile of the same pad for the SDL2 driver, under another name.
SDL2_DUALSENSE_PROFILE = (
    'input_driver = "sdl2"\n'
    'input_device = "PS5 Controller"\n'
    'input_vendor_id = "1356"\n'
    'input_product_id = "3302"\n'
    'input_b_btn = "0"\n'
)


def check(condition: bool, message: str) -> None:
    print(f"  {'ok  ' if condition else 'FAIL'} {message}")
    if not condition:
        FAILURES.append(message)


def tar_gz(path: Path, members: dict[str, str]) -> Path:
    """Make a source archive with `members`, each a path and its text."""
    with tarfile.open(path, "w:gz") as archive:
        for name, text in members.items():
            data = text.encode("utf-8")
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return path


def warned(action) -> tuple[object, str, BaseException | None]:
    """What `action` returned, what it wrote to stderr, and what it raised."""
    errors = io.StringIO()
    with contextlib.redirect_stderr(errors):
        try:
            return action(), errors.getvalue(), None
        except Exception as raised:  # noqa: BLE001 - the case reports it
            return None, errors.getvalue(), raised


def a_source_archive_without_its_licence_is_a_warning() -> None:
    with scratch.scratch("rominabox-preparation-") as folder:
        root = Path(folder)
        archive = tar_gz(root / "a5200-source.tar.gz", {"a5200-main/src/a5200.c": "int main;\n"})
        destination = root / "licenses" / "a5200.txt"
        destination.parent.mkdir()
        member, said, raised = warned(lambda: prepare_runtime.copy_license_from_source(
            archive, ("License.txt", "COPYING"), destination))
        check(raised is None, f"a source archive with no licence text is not refused (raised {raised!r})")
        check(member is None and not destination.exists(), "no licence text is written when there is none")
        check(destination.name in said and archive.name in said,
              f"the warning names the text and the archive: {said.strip()!r}")


def profiles_archive(root: Path, copying: str | None) -> Path:
    """Put the pinned autoconfig archive for this case where we look for it
    when staging."""
    revision = prepare_runtime.JOYPAD_AUTOCONFIG_REVISION
    top = f"retroarch-joypad-autoconfig-{revision}"
    members = {f"{top}/hid/{prepare_runtime.DUALSENSE_PROFILES['hid']}": DUALSENSE_PROFILE,
               f"{top}/sdl2/{prepare_runtime.DUALSENSE_PROFILES['sdl2']}": SDL2_DUALSENSE_PROFILE}
    if copying is not None:
        members[f"{top}/COPYING"] = copying
    return tar_gz(root / f"{prepare_runtime.JOYPAD_AUTOCONFIG_COMPONENT}-{revision}.tar.gz", members)


def stage_profiles(root: Path, copying: str | None) -> tuple[Path, str, BaseException | None]:
    """Stage the hid profiles from an archive whose COPYING is `copying`
    (none when None) into a kit under `root`, and return the output and
    exception of staging."""
    downloads = root / "downloads"
    downloads.mkdir()
    profiles_archive(downloads, copying)
    kit = root / "kit"
    (kit / "licenses").mkdir(parents=True)
    pinned = prepare_runtime.DOWNLOADS
    prepare_runtime.DOWNLOADS = downloads
    try:
        _, said, raised = warned(lambda: prepare_runtime.stage_joypad_autoconfig(kit, ["hid"]))
    finally:
        prepare_runtime.DOWNLOADS = pinned
    return kit, said, raised


def a_changed_upstream_licence_is_a_warning() -> None:
    with scratch.scratch("rominabox-preparation-") as folder:
        kit, said, raised = stage_profiles(Path(folder), "Some other licence, rewritten upstream.\n")
        check(raised is None, f"profiles whose upstream COPYING changed are not refused (raised {raised!r})")
        check((kit / "autoconfig/hid" / prepare_runtime.DUALSENSE_PROFILES["hid"]).is_file(),
              "the profiles are staged all the same")
        entry = prepare_runtime.licences.entry("data", prepare_runtime.JOYPAD_AUTOCONFIG_COMPONENT)
        check(entry.name in said, f"the warning names the licence entry: {said.strip()!r}")


def an_archive_without_a_licence_is_a_warning() -> None:
    with scratch.scratch("rominabox-preparation-") as folder:
        kit, said, raised = stage_profiles(Path(folder), None)
        check(raised is None, f"profiles from an archive with no COPYING are not refused (raised {raised!r})")
        check((kit / "autoconfig/hid" / prepare_runtime.DUALSENSE_PROFILES["hid"]).is_file(),
              "the profiles are staged all the same")
        check("COPYING" in said, f"the warning names the missing COPYING: {said.strip()!r}")


def a_driver_no_longer_staged_leaves_no_profiles() -> None:
    """Check that a kit staged for one controller driver and then for another
    contains only the profiles of the second, and not a folder left from the
    first."""
    with scratch.scratch("rominabox-preparation-") as folder:
        root = Path(folder)
        downloads = root / "downloads"
        downloads.mkdir()
        profiles_archive(downloads, "The upstream licence.\n")
        kit = root / "kit"
        (kit / "licenses").mkdir(parents=True)
        pinned = prepare_runtime.DOWNLOADS
        prepare_runtime.DOWNLOADS = downloads
        try:
            for drivers in (["hid"], ["sdl2"]):
                _, _said, raised = warned(lambda: prepare_runtime.stage_joypad_autoconfig(kit, drivers))
                check(raised is None, f"staging {drivers} succeeds (raised {raised!r})")
        finally:
            prepare_runtime.DOWNLOADS = pinned
        folders = sorted(path.name for path in (kit / "autoconfig").iterdir())
        check(folders == ["sdl2"], f"the kit holds the sdl2 profiles only, not {folders}")


def main() -> int:
    a_source_archive_without_its_licence_is_a_warning()
    a_changed_upstream_licence_is_a_warning()
    an_archive_without_a_licence_is_a_warning()
    a_driver_no_longer_staged_leaves_no_profiles()
    if FAILURES:
        print(f"\n{len(FAILURES)} preparation problem(s)")
        return 1
    print("\npreparation stages what a licence text says nothing about, and says what is missing or changed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
