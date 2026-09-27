"""Check that an automated launch makes no sound.

A launch by a person is unaffected. A shot, a quit run, and the isolation
and autoconfig launches are quiet. In the launcher we make every launch
quiet when its parent is not Launch Services. In a harness we can also set
the switch declared in menu_shots. In the plan check we compile the launcher,
write the config and exit before a core is loaded, so no audio opens. The
check also shows that the launcher and the harness use the same names for
the switch and its opt-out, which we never read from the launcher.

    python3 scripts/test_quiet.py
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import free_space  # noqa: E402
import menu_shots  # noqa: E402
import scratch  # noqa: E402
import toolchain  # noqa: E402
from launcher_plan import compile_plan  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# In place of the audio driver that we freeze into an export, we use a name
# that no RetroArch has. In the launcher we must keep the export's driver for
# a launch with sound, and replace it for a quiet launch.
FROZEN_DRIVER = "frozen-by-the-export"
# For a quiet launch we use RetroArch's null audio driver, with no device.
QUIET_DRIVER = "null"


LAUNCHER = ROOT / "desktop/src-tauri/launcher"
FORK = ROOT / "vendor/retroarch"


def shared_launcher_sources() -> list[str]:
    """Return the launcher sources common to every platform, without an entry point."""
    return sorted(str(path) for path in LAUNCHER.glob("*.c"))


def write_plan(resources: Path, data: Path, driver: str = FROZEN_DRIVER) -> None:
    """Return a launch plan with only the export's driver in its config."""
    resources.mkdir(parents=True, exist_ok=True)
    (resources / "launch.plan").write_bytes((
        "identity\tplan\n"
        "content\tcontent\n"
        "title\tPlan\n"
        f"data_dir\t{data}\n"
        "managed\tlogs\n"
        "\n---config---\n"
        f'audio_driver = "{driver}"\n'
        # What we write in an export when the player has not chosen to keep
        # playing in the background.
        'pause_nonactive = "true"\n'
    ).encode("utf-8"))


def run_plan(binary: Path, data: Path, quiet: bool = False, sound: bool = False) -> str:
    env = os.environ.copy()
    env["ROMINABOX_PLAN_ONLY"] = "1"
    # The parent process of the harness is not launchd. We remove both variables,
    # so that a value left in this process cannot hide the default.
    env.pop(menu_shots.QUIET_ENV, None)
    env.pop(menu_shots.SOUND_ENV, None)
    if quiet:
        env[menu_shots.QUIET_ENV] = "1"
    if sound:
        env[menu_shots.SOUND_ENV] = "1"
    ran = subprocess.run(
        [str(binary)],
        env=env,
        capture_output=True, text=True,
    )
    if ran.returncode != 0:
        raise SystemExit(
            f"the launcher plan tool exited {ran.returncode}\n{ran.stderr[-400:]}"
        )
    config = data / "retroarch.cfg"
    if not config.is_file():
        raise SystemExit(f"the launcher wrote no config at {config}")
    return config.read_text()


def plan_check() -> list[str]:
    """With the switch the frozen driver goes, and with the opt-out it stays.

    We make four plans and start no game. The harness is not Launch Services,
    so a launch with neither variable is quiet. With the opt-out alone the
    frozen driver stays, which shows that the launcher and the harness use the
    same name for the opt-out. With both, the switch takes precedence, which
    shows the same for the switch. With a different name in the launcher, the
    opt-out would take precedence and the frozen driver would stay.
    """
    free_space.require(20)
    with scratch.scratch("rominabox-quiet-plan-") as made:
        root = Path(made)
        binary, resources = compile_plan(root)
        written = {}
        for name, quiet, sound in (("neither", False, False), ("sound", False, True),
                                   ("switch", True, False), ("both", True, True)):
            data = root / name
            write_plan(resources, data)
            written[name] = run_plan(binary, data, quiet=quiet, sound=sound)
    failures = []
    expected = {"neither": QUIET_DRIVER, "sound": FROZEN_DRIVER,
                "switch": QUIET_DRIVER, "both": QUIET_DRIVER}
    for name, driver in expected.items():
        got = _config_value(written[name], "audio_driver")
        enabled = _config_value(written[name], "audio_enable")
        quiet = driver == QUIET_DRIVER
        if got != driver or (quiet and enabled != "false"):
            failures.append(
                f"a launch with {name} ({menu_shots.QUIET_ENV}, {menu_shots.SOUND_ENV}) wrote "
                f"audio_driver={got!r} audio_enable={enabled!r}, not {driver!r}"
                + (" disabled" if quiet else "")
            )
        # A quiet run is never in front, so a pause would stop it before its
        # frame limit. For a launch by a person we keep the player's choice.
        paused = _config_value(written[name], "pause_nonactive")
        if paused != ("false" if quiet else "true"):
            failures.append(
                f"a launch with {name} ({menu_shots.QUIET_ENV}, {menu_shots.SOUND_ENV}) wrote "
                f"pause_nonactive={paused!r}"
            )
    return failures


def _config_value(config: str, key: str) -> str | None:
    prefix = key + " = "
    for line in config.splitlines():
        if line.startswith(prefix):
            return line[len(prefix):].strip().strip('"')
    return None


def decision_check() -> list[str]:
    """Check the four cases by calling the function directly, with no config or core."""
    free_space.require(20)
    toolchain.activate()
    declared = toolchain.describe()
    with scratch.scratch("rominabox-quiet-decision-") as made:
        binary = toolchain.executable(Path(made) / "decision")
        compiled = subprocess.run(
            [
                declared["cc"], "-O2", "-o", str(binary),
                str(ROOT / "scripts/native_runtime/quiet_decision.c"), *shared_launcher_sources(),
            ],
            capture_output=True, text=True,
        )
        if compiled.returncode != 0:
            raise SystemExit(compiled.stderr[-400:] or "the decision tool did not compile")
        cases = (
            ("person", "", "", "sound"),
            ("harness", "", "", "quiet"),
            ("harness", "", "1", "sound"),
            ("person", "1", "", "quiet"),
        )
        failures = []
        for opener, quiet, sound, expect in cases:
            ran = subprocess.run(
                [str(binary), opener, quiet, sound],
                capture_output=True, text=True, timeout=15,
            )
            got = ran.stdout.strip()
            if ran.returncode != 0 or got != expect:
                failures.append(
                    f"a {opener}'s launch with quiet={quiet!r} sound={sound!r} "
                    f"decided {got!r}, not {expect!r}"
                )
        return failures


def macos_window_probe(binary: Path) -> Path:
    subprocess.run(
        [
            "clang", "-fobjc-arc", "-I", str(FORK), "-I", str(FORK / "libretro-common/include"),
            "-framework", "Cocoa", "-o", str(binary),
            str(ROOT / "scripts/native_runtime/test_quiet_window.m"),
            str(FORK / "rominabox_session.c"),
        ],
        check=True, capture_output=True, text=True,
    )
    return binary


def windows_window_probe(binary: Path) -> Path:
    toolchain.activate()
    declared = toolchain.describe()
    binary = toolchain.executable(binary)
    subprocess.run(
        [
            declared["cc"], "-std=gnu99", "-O2", "-I", str(FORK), "-I", str(FORK / "libretro-common/include"),
            "-I", str(ROOT / "scripts/native_runtime"), "-o", str(binary),
            str(ROOT / "scripts/native_runtime/test_quiet_window_win32.c"),
            str(FORK / "rominabox_session.c"),
            str(FORK / "libretro-common/encodings/encoding_utf.c"),
            str(FORK / "libretro-common/compat/compat_strl.c"),
        ],
        check=True, capture_output=True, text=True,
    )
    return binary


# The probe in which we prepare a window as we do in the player for an
# automated run, on each platform, and never show it.
WINDOW_PROBES = {"macos": macos_window_probe, "windows": windows_window_probe}


def window_visibility_check() -> list[str]:
    """Check transparency, mouse pass-through and the opt-out without showing a window.

    We check the drawn picture itself in the exported-game workflows.
    """
    if menu_shots.PLATFORM not in WINDOW_PROBES:
        raise SystemExit(f"no quiet window probe is declared for {menu_shots.PLATFORM}")
    with scratch.scratch("rominabox-quiet-window-") as made:
        binary = WINDOW_PROBES[menu_shots.PLATFORM](Path(made) / "window-visibility")
        checked = subprocess.run(
            [str(binary)], capture_output=True, text=True, timeout=15,
        )
        if checked.returncode:
            return [checked.stderr.strip() or "quiet window visibility check failed"]
    return []


def main() -> int:
    failures = decision_check()
    if not failures:
        failures.extend(plan_check())
    if not failures:
        failures.extend(window_visibility_check())
    if not failures:
        print("quiet: null audio, transparent noninteractive window, and explicit hands-on opt-outs")
        return 0
    print(f"\n{len(failures)} quiet check(s) failed:")
    for item in failures:
        print(f"  FAIL {item}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
