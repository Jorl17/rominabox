"""Check that the core options in an export reach the game's own options file.

In RetroArch, a core's options come from the game's data directory, not from
the app, so in the launcher we write the export's values there before
RetroArch starts. A file already in the data directory must not hide the
export's values, whether we copied it from an older data location, wrote it
in an earlier export of the same game, or it is a RetroArch rewrite with
every option of the core. When the player changed a value after we last
applied one in the launcher, we keep the player's value.

We compile the real launcher as its plan tool, in which we prepare the data
directory and exit before any core or window exists.

    python3 scripts/test_core_options.py
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import scratch  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
LAUNCHER = ROOT / "desktop/src-tauri/launcher"
CORE = "Nestopia"
FILTER = "nestopia_blargg_ntsc_filter"

# The file we get from RetroArch for a game exported without options, with
# every option of the core at its default.
RETROARCH_DEFAULTS = (
    'nestopia_aspect = "auto"\n'
    f'{FILTER} = "composite"\n'
    'nestopia_palette = "royaltea"\n'
)


def compile_plan(directory: Path) -> Path:
    binary = directory / "Plan.app" / "Contents" / "MacOS" / "plan"
    binary.parent.mkdir(parents=True)
    made = subprocess.run(
        ["cc", "-DROMINABOX_PLAN_MAIN", "-O2", "-o", str(binary),
         *sorted(str(path) for path in LAUNCHER.glob("*.c"))],
        capture_output=True, text=True,
    )
    if made.returncode != 0:
        raise SystemExit(made.stderr[-600:] or "the launcher plan tool did not compile")
    return binary


def ship_plan(app: Path, data: Path) -> None:
    """Return the export's launch plan, with `data` as its data directory."""
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
        'audio_driver = "null"\n'
    )


def ship(app: Path, data: Path, options: str) -> None:
    """Return the export, a launch plan and the options file we stage at packaging."""
    ship_plan(app, data)
    shipped = app / "Contents" / "Resources" / "core-options" / CORE
    shipped.mkdir(parents=True, exist_ok=True)
    (shipped / f"{CORE}.opt").write_text(options)


def launch(binary: Path, home: Path) -> None:
    env = os.environ.copy()
    env["ROMINABOX_PLAN_ONLY"] = "1"
    # Not below HOME, so we do not look for an earlier data location.
    env["HOME"] = str(home)
    ran = subprocess.run([str(binary)], env=env, capture_output=True, text=True, timeout=30)
    if ran.returncode != 0:
        raise SystemExit(f"the launcher plan tool exited {ran.returncode}\n{ran.stderr[-400:]}")


def values(path: Path) -> dict[str, str]:
    found = {}
    for line in path.read_text().splitlines():
        key, equals, value = line.partition("=")
        if equals:
            found[key.strip()] = value.strip().strip('"')
    return found


def run() -> list[str]:
    failures = []

    def expect(label: str, got: dict[str, str], key: str, wanted: str | None) -> None:
        if got.get(key) != wanted:
            failures.append(f"{label}: {key} is {got.get(key)!r}, expected {wanted!r}")

    with scratch.scratch("rominabox-core-options-") as made:
        root = Path(made)
        binary = compile_plan(root)
        app = binary.parents[2]
        home = root / "home"
        home.mkdir()

        # In a new data directory we write the export's value.
        fresh = root / "fresh"
        ship(app, fresh, f'{FILTER} = "disabled"\n')
        launch(binary, home)
        expect("fresh data", values(fresh / "config" / CORE / f"{CORE}.opt"), FILTER, "disabled")

        # A file that existed before we shipped the value in the export.
        data = root / "data"
        game_options = data / "config" / CORE / f"{CORE}.opt"
        game_options.parent.mkdir(parents=True)
        game_options.write_text(RETROARCH_DEFAULTS)
        ship(app, data, f'{FILTER} = "disabled"\n')
        launch(binary, home)
        got = values(game_options)
        expect("file from before the export", got, FILTER, "disabled")
        expect("file from before the export", got, "nestopia_palette", "royaltea")

        # In RetroArch the file is written again with every option when the
        # game closes. Launching the same export again changes nothing.
        game_options.write_text(RETROARCH_DEFAULTS.replace('"composite"', '"disabled"'))
        launch(binary, home)
        expect("relaunch", values(game_options), FILTER, "disabled")

        # The player changes it afterwards. We keep that value on this launch,
        # and after an export that ships the same value again.
        game_options.write_text(RETROARCH_DEFAULTS.replace('"composite"', '"svideo"'))
        launch(binary, home)
        expect("player's change", values(game_options), FILTER, "svideo")
        ship(app, data, f'{FILTER} = "disabled"\n')
        launch(binary, home)
        expect("player's change after re-export", values(game_options), FILTER, "svideo")

        # We give a new value from a new export to a player who never changed it.
        other = root / "other"
        ship(app, other, f'{FILTER} = "disabled"\n')
        launch(binary, home)
        other_options = other / "config" / CORE / f"{CORE}.opt"
        other_options.write_text(RETROARCH_DEFAULTS.replace('"composite"', '"disabled"'))
        ship(app, other, f'{FILTER} = "rgb"\n')
        launch(binary, home)
        expect("new export value", values(other_options), FILTER, "rgb")

        # When an export no longer sets the option, the core's default applies.
        ship(app, other, 'nestopia_aspect = "4:3"\n')
        launch(binary, home)
        got = values(other_options)
        expect("option no longer shipped", got, FILTER, None)
        expect("option newly shipped", got, "nestopia_aspect", "4:3")

        # With an export that has no options at all, every option set by an
        # earlier export goes back to the core's default. We remove only the
        # shipped file and its folders, each by name, never recursively.
        resources = app / "Contents" / "Resources"
        (resources / "core-options" / CORE / f"{CORE}.opt").unlink()
        (resources / "core-options" / CORE).rmdir()
        (resources / "core-options").rmdir()
        launch(binary, home)
        expect("no options shipped", values(other_options), "nestopia_aspect", None)

        # We still copy a shipped file without assignments to a game that has
        # no file, as with the copy it replaced.
        probe = resources / "core-options" / "probe-core"
        probe.mkdir(parents=True)
        (probe / "copied.cfg").write_text("copied-from-kit\n")
        plain = root / "plain"
        ship_plan(app, plain)
        launch(binary, home)
        copied = plain / "config" / "probe-core" / "copied.cfg"
        if not copied.is_file() or copied.read_text() != "copied-from-kit\n":
            failures.append("a shipped file without assignments did not reach a fresh game")
    return failures


def main() -> int:
    failures = run()
    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print("core options: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
