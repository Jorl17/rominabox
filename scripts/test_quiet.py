"""Check that an automated launch makes no sound.

A launch by a person is unaffected. A shot, a quit run, and the isolation
and autoconfig launches set one switch, whose name is declared in the
launcher. In the plan check we compile the launcher, write the config and
exit before a core is loaded, so we open no audio device.

    python3 scripts/test_quiet.py
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import menu_shots  # noqa: E402
import scratch  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# Sources in which we start the exported player. We leave out frame_harness,
# with which we run a core without RetroArch or an audio device.
LAUNCH_SOURCES = (
    ROOT / "scripts/menu_shots.py",
    ROOT / "scripts/test_quit.py",
    ROOT / "desktop/src-tauri/tests/isolation.rs",
    ROOT / "desktop/src-tauri/tests/input_autoconfig.rs",
)


def free_gb() -> float:
    line = subprocess.run(
        ["df", "-k", "/Users/mariowilde"], capture_output=True, text=True, check=True
    ).stdout.splitlines()[1]
    return int(line.split()[3]) / 1024 / 1024


def quiet_name() -> str | None:
    """Return the switch as spelled in the launcher, or None when it is absent."""
    found = menu_shots.QUIET_ENV.search(
        (ROOT / "desktop/src-tauri/launcher/main.c").read_text()
    )
    return found.group(1) if found else None


def shipped_driver() -> str:
    """Return the driver we freeze into an export, from its one line in packaging."""
    text = (ROOT / "desktop/src-tauri/src/packaging.rs").read_text()
    found = re.search(r'^audio_driver = "([^"]+)"', text, re.MULTILINE)
    if not found:
        raise SystemExit("the export does not name an audio driver")
    return found.group(1)


def forced_driver() -> str | None:
    """Return the driver in the quiet block, or None when that line is absent."""
    text = (ROOT / "desktop/src-tauri/launcher/main.c").read_text()
    found = re.search(
        r'"audio_driver", "audio_driver = \\"([^"\\]+)\\"',
        text,
    )
    return found.group(1) if found else None


def compile_plan(directory: Path) -> Path:
    binary = directory / "Plan.app" / "Contents" / "MacOS" / "plan"
    binary.parent.mkdir(parents=True)
    made = subprocess.run(
        [
            "cc", "-DROMINABOX_PLAN_MAIN", "-O2",
            "-o", str(binary),
            str(ROOT / "desktop/src-tauri/launcher/main.c"),
        ],
        capture_output=True, text=True,
    )
    if made.returncode != 0:
        raise SystemExit(made.stderr[-400:] or "the launcher plan tool did not compile")
    return binary


def write_plan(app: Path, data: Path, driver: str) -> None:
    """Return a launch plan with only the shipped driver in its config."""
    resources = app / "Contents" / "Resources"
    resources.mkdir(parents=True, exist_ok=True)
    (resources / "launch.plan").write_text(
        "identity\tplan\n"
        "content\tcontent\n"
        "title\tPlan\n"
        "volume_file\tvolume.cfg\n"
        f"data_dir\t{data}\n"
        "managed\tlogs\n"
        "\n---config---\n"
        f'audio_driver = "{driver}"\n'
    )


def sound_name() -> str:
    """Return the opt-out as it is spelled once in the launcher."""
    found = re.search(
        r'#define ROMINABOX_SOUND_ENV "([A-Z0-9_]+)"',
        (ROOT / "desktop/src-tauri/launcher/main.c").read_text(),
    )
    if not found:
        raise SystemExit("launcher does not declare ROMINABOX_SOUND_ENV")
    return found.group(1)


def run_plan(binary: Path, data: Path, quiet: str | None, sound: bool = False) -> str:
    env = os.environ.copy()
    env["ROMINABOX_PLAN_ONLY"] = "1"
    # The parent process of the harness is not launchd. We remove both variables,
    # so that a value left in this process cannot hide the default.
    for key in (quiet_name(), sound_name()):
        if key:
            env.pop(key, None)
    if quiet:
        env[quiet] = "1"
    if sound:
        env[sound_name()] = "1"
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
    """Check that we replace the frozen driver when the switch is set. No game starts."""
    shipped = shipped_driver()
    forced = forced_driver()
    name = quiet_name()
    if not forced or not name:
        return ["the launcher does not force a quiet audio driver"]
    if free_gb() < 20:
        raise SystemExit(f"disk has {free_gb():.1f} GB free, below 20; stopping")
    with scratch.scratch("rominabox-quiet-plan-") as made:
        root = Path(made)
        binary = compile_plan(root)
        app = binary.parents[2]
        loud = root / "loud"
        asked = root / "sound"
        silent = root / "silent"
        write_plan(app, loud, shipped)
        without = run_plan(binary, loud, None)
        write_plan(app, asked, shipped)
        with_sound = run_plan(binary, asked, None, sound=True)
        write_plan(app, silent, shipped)
        with_switch = run_plan(binary, silent, name)
    failures = []
    # The harness is not Launch Services, so a launch with neither variable
    # is quiet. In an export whose launcher does not read ROMINABOX_QUIET, the
    # variable has no effect and there is sound.
    got = _config_value(without, "audio_driver")
    if got != forced:
        failures.append(
            f"a harness launch with neither variable wrote audio_driver={got!r}, "
            f"not {forced!r}"
        )
    got = _config_value(with_sound, "audio_driver")
    if got != shipped:
        failures.append(
            f"{sound_name()} wrote audio_driver={got!r}, not the shipped {shipped!r}"
        )
    got = _config_value(with_switch, "audio_driver")
    enabled = _config_value(with_switch, "audio_enable")
    if got != forced or enabled != "false":
        failures.append(
            f"with the switch the launcher wrote audio_driver={got!r} "
            f"audio_enable={enabled!r}, not {forced!r} disabled"
        )
    return failures


def _config_value(config: str, key: str) -> str | None:
    prefix = key + " = "
    for line in config.splitlines():
        if line.startswith(prefix):
            return line[len(prefix):].strip().strip('"')
    return None


def functions_of(text: str) -> list[tuple[str, str]]:
    """Return the top-level Python `def` and Rust `fn` bodies, one per function."""
    marks = [(match.start(), match.group(1)) for match in re.finditer(
        r"^(?:def|fn) ([A-Za-z0-9_]+)", text, re.MULTILINE,
    )]
    bodies = []
    for index, (start, name) in enumerate(marks):
        end = marks[index + 1][0] if index + 1 < len(marks) else len(text)
        bodies.append((name, text[start:end]))
    return bodies


def launches_player(body: str) -> bool:
    """Return True when we start the exported player in this function."""
    if 'Contents/MacOS/retroarch' in body and "Command::new" in body:
        return True
    if "Command::new(&launcher)" in body or "Command::new(launcher)" in body:
        return True
    if "launcher_of(app)" not in body:
        return False
    if "lldb" in body:
        return True
    # The binary's path in a codesign command is not a launch.
    if "subprocess.run" in body or "Popen" in body:
        return "codesign" not in body
    return False


def sets_switch(name: str, body: str, bodies: list[tuple[str, str]]) -> bool:
    """Return True if the launch or a function in this file that it calls contains the switch.

    In Python we read the spelling through quiet_env() and never copy the string.
    """
    if name in body or "quiet_env()" in body:
        return True
    for other, other_body in bodies:
        if f"{other}(" in body and name in other_body:
            return True
    return False


def guardrail() -> list[str]:
    """Check that we set the one declared switch in every launch of the player."""
    name = quiet_name()
    if not name:
        return ["the launcher does not declare ROMINABOX_QUIET_ENV"]
    missing = []
    for source in LAUNCH_SOURCES:
        text = source.read_text()
        bodies = functions_of(text)
        for function, body in bodies:
            if not launches_player(body):
                continue
            if not sets_switch(name, body, bodies):
                missing.append(
                    f"{source.relative_to(ROOT)} {function} launches the player "
                    f"without {name}"
                )
    harness = (ROOT / "scripts/native_runtime/frame_harness.c").read_text()
    if "AudioOutputUnit" in harness or "kAudioUnit" in harness:
        missing.append("frame_harness opens an audio unit; it used to load a core with no device")
    return missing


def decision_check() -> list[str]:
    """Check the four cases by calling the function directly, with no config or core."""
    if free_gb() < 20:
        raise SystemExit(f"disk has {free_gb():.1f} GB free, below 20; stopping")
    with scratch.scratch("rominabox-quiet-decision-") as made:
        binary = Path(made) / "decision"
        compiled = subprocess.run(
            [
                "cc", "-DROMINABOX_DECISION_MAIN", "-O2",
                "-o", str(binary),
                str(ROOT / "desktop/src-tauri/launcher/main.c"),
            ],
            capture_output=True, text=True,
        )
        if compiled.returncode != 0:
            raise SystemExit(compiled.stderr[-400:] or "the decision tool did not compile")
        cases = (
            ("1", "", "", "sound"),
            ("20", "", "", "quiet"),
            ("20", "", "1", "sound"),
            ("1", "1", "", "quiet"),
        )
        failures = []
        for parent, quiet, sound, expect in cases:
            ran = subprocess.run(
                [str(binary), parent, quiet, sound],
                capture_output=True, text=True, timeout=15,
            )
            got = ran.stdout.strip()
            if ran.returncode != 0 or got != expect:
                failures.append(
                    f"parent {parent} quiet={quiet!r} sound={sound!r} "
                    f"decided {got!r}, not {expect!r}"
                )
        return failures


def main() -> int:
    # We run this check first, so that we report a launch without the switch
    # before we execute anything.
    failures = guardrail()
    if not failures:
        failures.extend(decision_check())
    if not failures:
        failures.extend(plan_check())
    if not failures:
        print("quiet: the switch forces a null audio driver, and every launch sets it")
        return 0
    print(f"\n{len(failures)} quiet check(s) failed:")
    for item in failures:
        print(f"  FAIL {item}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
