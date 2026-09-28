"""Check that the core options in an export reach the game's options file,
and that the settings a player chose in the game's menu reach the next launch.

The options of a core come from the game's data directory, not from the app,
so in the launcher we put the export's values there before RetroArch starts.
A file already in the data directory must not hide the export's values,
whether it is a copy from an older data location, a file from an earlier
export of the same game, or a file that RetroArch rewrote with every option
of the core. A value that the player changed after we last set it stays.

For a player setting it is the other way round, and in the export we only
set the default. On the next launch we give RetroArch the value that the
player chose in the game's menu, from a separate file in the game's data,
whatever the default in a later export. A player who never chose gets the
default from the export.

We compile the launcher as a plan tool, with which we prepare the data
directory and exit before any core or window exists. On macOS this is the
plan entry, and on Windows the game's launcher, stopped by ROMINABOX_PLAN_ONLY.

    python3 scripts/test_shipped.py
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
import scratch  # noqa: E402
from launcher_plan import compile_plan  # noqa: E402
from menu_shots import QUIET_ENV, SOUND_ENV  # noqa: E402

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


def write(path: Path, text: str) -> None:
    """Return `text` in the file format of the exporter and RetroArch: UTF-8,
    with LF line ends on every platform."""
    path.write_bytes(text.encode("utf-8"))


# The lines in which we declare the player settings in an export's plan, as
# we write them in packaging: file, key, default.
PLAYER_SETTINGS = (
    "player_setting\tvolume.cfg\taudio_volume\t0.0\n"
    "player_setting\tbackground-play.cfg\tpause_nonactive\t{pause}\n"
    "player_setting\trumble.cfg\tinput_rumble_enable\ttrue\n"
)


def ship_plan(resources: Path, data: Path | str, pause_nonactive: str = "true",
              managed: tuple[str, ...] = ("logs",)) -> None:
    """Return the export's launch plan, with `data` as its data directory."""
    resources.mkdir(parents=True, exist_ok=True)
    write(
        resources / "launch.plan",
        "identity\tplan\n"
        "content\tcontent\n"
        "title\tPlan\n"
        + PLAYER_SETTINGS.format(pause=pause_nonactive)
        + f"data_dir\t{data}\n"
        + "".join(f"managed\t{name}\n" for name in managed)
        + "\n---config---\n"
        'audio_driver = "null"\n'
    )


def ship(resources: Path, data: Path, options: str) -> None:
    """Return the export, a launch plan and the options file we stage at packaging."""
    ship_plan(resources, data)
    shipped = resources / "core-options" / CORE
    shipped.mkdir(parents=True, exist_ok=True)
    write(shipped / f"{CORE}.opt", options)


def launch(binary: Path, home: Path) -> None:
    ran = plan_tool(binary, home)
    if ran.returncode != 0:
        raise SystemExit(f"the launcher plan tool exited {ran.returncode}\n{ran.stderr[-400:]}")


def plan_tool(binary: Path, home: Path) -> subprocess.CompletedProcess:
    env = os.environ.copy()
    env["ROMINABOX_PLAN_ONLY"] = "1"
    # Not below HOME, so that on macOS we look for no earlier data location.
    env["HOME"] = str(home)
    # A launch by a person. This harness is not Launch Services, so otherwise
    # we would treat it in the launcher as an automated run, which never
    # pauses in the background and so hides the player's background-play choice.
    env.pop(QUIET_ENV, None)
    env[SOUND_ENV] = "1"
    return subprocess.run([str(binary)], env=env, capture_output=True, text=True, timeout=30)


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
        binary, resources = compile_plan(root)
        home = root / "home"
        home.mkdir()

        # In a new data directory we write the export's value.
        fresh = root / "fresh"
        ship(resources, fresh, f'{FILTER} = "disabled"\n')
        launch(binary, home)
        expect("fresh data", values(fresh / "config" / CORE / f"{CORE}.opt"), FILTER, "disabled")

        # A file that existed before we shipped the value in the export.
        data = root / "data"
        game_options = data / "config" / CORE / f"{CORE}.opt"
        game_options.parent.mkdir(parents=True)
        write(game_options, RETROARCH_DEFAULTS)
        ship(resources, data, f'{FILTER} = "disabled"\n')
        launch(binary, home)
        got = values(game_options)
        expect("file from before the export", got, FILTER, "disabled")
        expect("file from before the export", got, "nestopia_palette", "royaltea")

        # In RetroArch the file is written again with every option when the
        # game closes. Launching the same export again changes nothing.
        write(game_options, RETROARCH_DEFAULTS.replace('"composite"', '"disabled"'))
        launch(binary, home)
        expect("relaunch", values(game_options), FILTER, "disabled")

        # The player changes it afterwards. We keep that value on this launch,
        # and after an export that ships the same value again.
        write(game_options, RETROARCH_DEFAULTS.replace('"composite"', '"svideo"'))
        launch(binary, home)
        expect("player's change", values(game_options), FILTER, "svideo")
        ship(resources, data, f'{FILTER} = "disabled"\n')
        launch(binary, home)
        expect("player's change after re-export", values(game_options), FILTER, "svideo")

        # We give a new value from a new export to a player who never changed it.
        other = root / "other"
        ship(resources, other, f'{FILTER} = "disabled"\n')
        launch(binary, home)
        other_options = other / "config" / CORE / f"{CORE}.opt"
        write(other_options, RETROARCH_DEFAULTS.replace('"composite"', '"disabled"'))
        ship(resources, other, f'{FILTER} = "rgb"\n')
        launch(binary, home)
        expect("new export value", values(other_options), FILTER, "rgb")

        # When an export no longer sets the option, the core's default applies.
        ship(resources, other, 'nestopia_aspect = "4:3"\n')
        launch(binary, home)
        got = values(other_options)
        expect("option no longer shipped", got, FILTER, None)
        expect("option newly shipped", got, "nestopia_aspect", "4:3")

        # With an export that has no options at all, every option set by an
        # earlier export goes back to the core's default. We remove only the
        # shipped file and its folders, each by name, never recursively.
        (resources / "core-options" / CORE / f"{CORE}.opt").unlink()
        (resources / "core-options" / CORE).rmdir()
        (resources / "core-options").rmdir()
        launch(binary, home)
        expect("no options shipped", values(other_options), "nestopia_aspect", None)

        # We still copy a shipped file without assignments to a game that has
        # no file, as with the copy it replaced.
        probe = resources / "core-options" / "probe-core"
        probe.mkdir(parents=True)
        write(probe / "copied.cfg", "copied-from-kit\n")
        plain = root / "plain"
        ship_plan(resources, plain)
        launch(binary, home)
        copied = plain / "config" / "probe-core" / "copied.cfg"
        if not copied.is_file() or copied.read_text() != "copied-from-kit\n":
            failures.append("a shipped file without assignments did not reach a fresh game")
        (probe / "copied.cfg").unlink()
        probe.rmdir()
        (resources / "core-options").rmdir()

        # The same rule as for core options applies to remaps and controller
        # profiles. A file left by an earlier export does not replace the new app.
        pads = root / "pads"
        remap = pads / "remaps" / "Genesis Plus GX" / "Genesis Plus GX.rmp"
        profile = pads / "autoconfig" / "hid" / "Pad.cfg"
        for leftover, text in ((remap, 'input_libretro_device_p1 = "1"\n'),
                               (profile, 'input_b_btn = "1"\n')):
            leftover.parent.mkdir(parents=True)
            write(leftover, text)
        ship_plan(resources, pads)
        shipped_remap = resources / "remaps" / "Genesis Plus GX" / "Genesis Plus GX.rmp"
        shipped_profile = resources / "autoconfig" / "hid" / "Pad.cfg"
        for shipped, text in ((shipped_remap, 'input_libretro_device_p1 = "513"\n'),
                              (shipped_profile, 'input_b_btn = "2"\n')):
            shipped.parent.mkdir(parents=True)
            write(shipped, text)
        launch(binary, home)
        expect("remap from an earlier export", values(remap), "input_libretro_device_p1", "513")
        expect("controller profile from an earlier export", values(profile), "input_b_btn", "2")

        # The person who builds the game chooses the firmware, never the player.
        # We replace a different file with the shipped one, and remove a file
        # from an earlier export when this one has none. A file from a core stays.
        bios = root / "bios"
        system = bios / "system"
        system.mkdir(parents=True)
        (system / "core-made.dat").write_bytes(b"core")
        ship_plan(resources, bios)
        firmware = resources / "firmware"
        firmware.mkdir()
        (firmware / "scph1001.bin").write_bytes(b"first")
        launch(binary, home)
        (firmware / "scph1001.bin").unlink()
        (firmware / "scph5501.bin").write_bytes(b"second")
        (system / "scph5501.bin").write_bytes(b"left over")
        launch(binary, home)
        if (system / "scph5501.bin").read_bytes() != b"second":
            failures.append("firmware: a different file in the game was not replaced")
        if (system / "scph1001.bin").exists():
            failures.append("firmware: a file the earlier export shipped was left behind")
        if (system / "core-made.dat").read_bytes() != b"core":
            failures.append("firmware: a file the core wrote was touched")
    return failures


def run_player_settings() -> list[str]:
    """Return the value we give RetroArch for a setting the player changes in the menu."""
    failures = []

    def expect(label: str, data: Path, key: str, wanted: str | None) -> None:
        got = values(data / "retroarch.cfg").get(key)
        if got != wanted:
            failures.append(f"{label}: {key} is {got!r}, expected {wanted!r}")

    with scratch.scratch("rominabox-player-settings-") as made:
        root = Path(made)
        binary, resources = compile_plan(root)
        home = root / "home"
        home.mkdir()
        data = root / "data"

        # With nothing chosen, each setting has the export's default.
        ship_plan(resources, data, pause_nonactive="true")
        launch(binary, home)
        expect("no choice", data, "pause_nonactive", "true")
        expect("no choice", data, "audio_volume", "0.0")
        expect("no choice", data, "input_rumble_enable", "true")
        if (data / "background-play.cfg").exists():
            failures.append("the export's default was written into the player's file")

        # A player who never chose gets the new default from a new export.
        ship_plan(resources, data, pause_nonactive="false")
        launch(binary, home)
        expect("new default, no choice", data, "pause_nonactive", "false")

        # The player chooses in the menu, and we write the setting's file.
        write(data / "background-play.cfg", 'pause_nonactive = "true"\n')
        write(data / "volume.cfg", 'audio_volume = "-35.6"\n')
        write(data / "rumble.cfg", 'input_rumble_enable = "false"\n')
        launch(binary, home)
        expect("the player's choice", data, "pause_nonactive", "true")
        expect("the player's choice", data, "audio_volume", "-35.6")
        expect("the player's choice", data, "input_rumble_enable", "false")

        # After a re-export with the other default, the player's choice stays.
        ship_plan(resources, data, pause_nonactive="false")
        launch(binary, home)
        expect("re-export after a choice", data, "pause_nonactive", "true")
        if values(data / "background-play.cfg").get("pause_nonactive") != "true":
            failures.append("a re-export changed the player's own file")

        # Only the setting's key comes from its file.
        write(
            data / "background-play.cfg",
            'pause_nonactive = "false"\nvideo_driver = "vulkan"\n')
        launch(binary, home)
        expect("a stray key in a setting's file", data, "video_driver", None)
        expect("a stray key in a setting's file", data, "pause_nonactive", "false")
    return failures


def run_plan_places() -> list[str]:
    """Check that the data folder and managed folders of a plan stay in bounds.

    We refuse the content path when it starts at a root or climbs out with
    `..`, and apply the same rule to the data folder and every managed folder.
    For a refused plan we stop before we make anything, so the place in the
    plan is never created.
    """
    failures = []
    with scratch.scratch("rominabox-plan-places-") as made:
        root = Path(made)
        binary, resources = compile_plan(root)
        home = root / "home"
        home.mkdir()
        data = root / "data"
        # Each case contains the data folder, the managed folders, and where we
        # would write if we followed it. On Windows, $user_data is the person's
        # per-user folder, which HOME does not move, so those cases are macOS only.
        cases = {
            "an absolute data folder climbing out": (f"{data}/../escaped-data", ("logs",), root / "escaped-data"),
            "a managed folder climbing out": (data, ("logs", "../escaped-managed"), root / "escaped-managed"),
            # Joined to the data folder, this path is inside it on macOS and is
            # no folder at all on Windows. We check for the refusal.
            "a managed folder from the root": (data, ("logs", str(root / "rooted")), None),
        }
        if sys.platform == "darwin":
            cases["a data folder climbing out of $user_data"] = (
                "$user_data/../../../escaped-user-data", ("logs",), root / "escaped-user-data")
        for label, (data_dir, managed, escaped) in cases.items():
            ship_plan(resources, data_dir, managed=managed)
            ran = plan_tool(binary, home)
            if ran.returncode == 0:
                failures.append(f"{label}: the launcher was not refused")
            elif "leaves" not in ran.stderr:
                failures.append(f"{label}: refused for another reason: {ran.stderr.strip()[-300:]}")
            if escaped is not None and escaped.exists():
                failures.append(f"{label}: the launcher made {escaped}")

        # A plan as we write it in an export still launches, in the per-user folder.
        if sys.platform == "darwin":
            ship_plan(resources, "$user_data/ROM-in-a-Box/Games/plan", managed=("logs", "overlays/keyboards"))
            ran = plan_tool(binary, home)
            games = home / "Library/Application Support/ROM-in-a-Box/Games/plan"
            if ran.returncode != 0:
                failures.append(f"an export's plan was refused: {ran.stderr.strip()[-300:]}")
            elif not (games / "retroarch.cfg").is_file() or not (games / "overlays/keyboards").is_dir():
                failures.append(f"an export's plan did not make its data folder at {games}")
    return failures


def run_menu_sounds() -> list[str]:
    """Check that the menu sounds are in the folder that we give RetroArch in an
    exported game.

    When the game starts, the menu's OK, Cancel and movement sounds come from
    `assets_directory/sounds`, and no other sound plays. We prepare an export
    with a sound pack with the launcher, in which we write the game's config
    and stop before RetroArch. The folder in the config must be the folder
    where we put the pack in the export.
    """
    import size_bundles  # noqa: E402

    with scratch.scratch("rominabox-menu-sounds-") as made:
        root = Path(made)
        rom = root / "stand-in.bin"
        rom.write_bytes(b"RIBsounds")
        app = size_bundles.export(
            size_bundles.cli(), "shipped-menu-sounds", size_bundles.KIT,
            size_bundles.core_cache(), rom, {"menuSounds": "blip"},
        )
        shipped = size_bundles.resources(app) / "assets" / "sounds"
        if not (shipped / "ok.wav").is_file():
            return [f"the export shipped no pack at {shipped}"]
        binary, resources = compile_plan(root)
        shutil.copytree(size_bundles.resources(app), resources, dirs_exist_ok=True)
        size_bundles.remove_owned(app.parent)
        # The export's data folder is in the person's application data, but
        # for this run we keep the data here. A game in its sandbox could not
        # write here, so we run the plan tool outside it, as we run an unsigned
        # Mac one whatever its plan contains.
        data = root / "data"
        plan = resources / "launch.plan"
        write(plan, "".join(
            f"data_dir\t{data}\n" if line.startswith("data_dir\t") else line
            for line in plan.read_text(encoding="utf-8").splitlines(keepends=True)
            if not line.startswith("sandbox\t")))
        home = root / "home"
        home.mkdir()
        launch(binary, home)
        written = data / "retroarch.cfg"
        if not written.is_file():
            return ["the launcher wrote no retroarch.cfg"]
        assets = values(written).get("assets_directory", "")
        told = Path(assets) / "sounds" / "ok.wav"
        if not told.is_file():
            return [f"menu sounds: RetroArch is told they are in {assets}/sounds, which does not "
                    f"hold the pack (it is in {resources / 'assets/sounds'})"]
    return []


# The modules that a Windows launcher will share, beside its file layer. We
# cannot run them on Windows here, but we check that they compile there, so
# that no POSIX-only call can slip in.
PORTABLE = ("launch.c", "shipped_settings.c", "shipped_files.c", "accounts_folder.c", "player_settings.c")


def windows_build(directory: Path) -> list[str]:
    zig = shutil.which("zig")
    if not zig:
        print("core options: Windows build not checked, zig is not installed")
        return []
    failures = []
    for source in [*(LAUNCHER / name for name in PORTABLE), *native_build.file_layer("windows")]:
        name = source.relative_to(LAUNCHER).as_posix()
        built = subprocess.run(
            [zig, "cc", "-target", "x86_64-windows-gnu", "-Wall", "-Wextra", "-Werror",
             "-c", str(source), "-o", str(directory / f"{name.replace('/', '-')}.obj")],
            capture_output=True, text=True,
        )
        if built.returncode != 0:
            failures.append(f"{name} does not build for Windows:\n{built.stderr[-600:]}")
    return failures


def main() -> int:
    failures = run() + run_player_settings() + run_plan_places() + run_menu_sounds()
    with scratch.scratch("rominabox-core-options-windows-") as made:
        failures += windows_build(Path(made))
    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print("core options: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
