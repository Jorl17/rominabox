"""Check that in a game's player we use only its data folder, refuse to start
without one, and read the arguments as UTF-8 on Windows, and that a header
removed from the fork does not stop the next build of an earlier build folder.

Stock RetroArch fails open. With no folder given, the Windows build created
about 25 folders beside the program and would read the user's configuration.
A game must only use the absolute data folder that we give in
ROMINABOX_DATA_DIR in its launcher. We ask the player only for its feature
list, which we print before any window or core, so we open no window here.

    ROMINABOX_TEST_BUILD=/absolute/build python3 scripts/test_player.py
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_build  # noqa: E402
import player_build  # noqa: E402

REFUSAL = "ROMINABOX_DATA_DIR must be set to an absolute path"


def ask_features(player: Path, data_dir: str | None) -> subprocess.CompletedProcess:
    environment = {name: value for name, value in os.environ.items() if not name.startswith("ROMINABOX_")}
    if data_dir is not None:
        environment["ROMINABOX_DATA_DIR"] = data_dir
    return subprocess.run([str(player), "--features"], cwd=player.parent, env=environment,
                          capture_output=True, text=True, errors="replace", timeout=60)


def utf8_arguments(build: Path, player: Path) -> int:
    """Check that we declare UTF-8 as the code page of the player on Windows.

    The arguments arrive in the code page of the process, and from the
    launcher we pass paths under the game's folder and the player's folder.
    Without UTF-8, a game called Pokémon or a user with a non-ASCII name makes
    every path unreadable. The code page applies to the whole process, so it
    also covers a core when it opens its content with the C runtime. We read
    it from the player's manifest, because a launch that proves it opens a window.
    """
    target = player_build.info(build).get("target", "")
    if not target.startswith("windows-"):
        print(f"ok {target} passes arguments as UTF-8 already")
        return 0
    body = player.read_bytes()
    start = body.find(b"<assembly")
    end = body.find(b"</assembly>", start)
    manifest = body[start:end].decode("utf-8", "replace") if start >= 0 and end > start else ""
    if "<activeCodePage" not in manifest or ">UTF-8</activeCodePage>" not in manifest:
        print(f"FAIL the Windows player does not declare UTF-8 as its code page; its manifest:\n{manifest or '(none)'}")
        return 1
    if 'level="asInvoker"' not in manifest:
        print("FAIL the Windows player's manifest lost asInvoker")
        return 1
    print("ok the Windows player declares UTF-8 as its code page")
    return 0


# The kinds of source that we compile in the fork's makefile with a dependency
# file for every target, each with its own rule.
COMPILED_KINDS = (".c", ".cpp")


def dependency_rule(dependencies: str) -> tuple[str, list[str]]:
    """Return the object of a dependency file and the files that it lists, with
    its source first and then the headers that it was compiled with."""
    rule = dependencies.replace("\\\n", " ").splitlines()[0]
    target, _, prerequisites = rule.partition(":")
    return target.strip(), prerequisites.split()


def built_with_a_fork_header(retroarch: Path) -> list[tuple[str, Path, str]]:
    """Return, for each kind of source compiled in the folder, one object, its
    dependency file and a header of the fork that it was compiled with."""
    found: dict[str, tuple[str, Path, str]] = {}
    for dependencies in sorted(retroarch.glob("obj-*/**/*.d")):
        target, named = dependency_rule(dependencies.read_text(encoding="utf-8", errors="replace"))
        kind = Path(named[0]).suffix if named else ""
        if kind not in COMPILED_KINDS or kind in found:
            continue
        header = next((name for name in named[1:]
                       if Path(name).suffix in (".h", ".hpp") and not name.startswith(("../", "/"))
                       and (retroarch / name).is_file()), None)
        if header:
            found[kind] = (target, dependencies, header)
    return [found[kind] for kind in COMPILED_KINDS if kind in found]


def a_removed_header_does_not_stop_the_next_build(build: Path) -> int:
    """Check that make does not stop when a header used for an object has since
    gone from the fork, and that we compile the object again.

    A renamed header must not stop the next build of a folder built before
    the rename with "No rule to make target". For each kind of source we
    rename one header of its object to a name that the fork does not have, in
    a copy of the dependency file from the build, and ask make what it would
    do for that object. We change nothing in the build folder.
    """
    target = player_build.info(build).get("target", "")
    parts = native_build.slices(target)
    failures = 0
    for part in parts:
        folder = build if parts == [target] else build / part
        retroarch = folder / "retroarch"
        chosen = built_with_a_fork_header(retroarch)
        if len(chosen) != len(COMPILED_KINDS):
            print(f"FAIL {retroarch} has no dependency file naming a fork header for every one of {COMPILED_KINDS}")
            failures += 1
            continue
        environment = {**native_build.build_environment(part), **native_build.freetype_environment(folder)}
        with tempfile.TemporaryDirectory(prefix="rominabox-removed-header-") as temporary:
            command = [native_build.resolve("make", environment), "-n", "-f", "Makefile"]
            for index, (_, dependencies, header) in enumerate(chosen):
                gone = f"{Path(header).with_suffix('').as_posix()}.removed-since-this-build{Path(header).suffix}"
                copy = Path(temporary) / f"{index}.d"
                copy.write_text(re.sub(rf"(?<!\S){re.escape(header)}(?=[\s:]|$)", gone,
                                       dependencies.read_text(encoding="utf-8", errors="replace")),
                                encoding="utf-8")
                command += ["-f", native_build.make_path(copy, part)]
            objects = [object_ for object_, _, _ in chosen]
            ran = subprocess.run([*command, *objects], cwd=retroarch, env=environment,
                                 capture_output=True, text=True, errors="replace")
        planned = [object_ for object_ in objects if f"-o {object_} " in ran.stdout]
        if ran.returncode != 0 or planned != objects:
            print(f"FAIL with a header gone from the fork, make for {', '.join(objects)} in {retroarch} "
                  f"(exit {ran.returncode}) planned to compile {planned or 'nothing'}:\n{ran.stderr[-1500:]}")
            failures += 1
        else:
            print(f"ok with a header gone from the fork, make compiles {', '.join(objects)} again in {part}")
    return failures


def main() -> int:
    build = player_build.selected_build()
    source = player_build.player_in(build)
    failures = utf8_arguments(build, source)
    failures += a_removed_header_does_not_stop_the_next_build(build)
    with tempfile.TemporaryDirectory(prefix="rominabox-data-root-") as temporary:
        # The player alone in a folder, so that we see anything created beside it.
        folder = Path(temporary) / "bin"
        folder.mkdir()
        player = folder / source.name
        shutil.copy2(source, player)
        data = Path(temporary) / "data"
        data.mkdir()

        for label, data_dir in (("no data folder", None), ("a relative data folder", "data")):
            ran = ask_features(player, data_dir)
            output = ran.stdout + ran.stderr
            beside = sorted(entry.name for entry in folder.iterdir() if entry != player)
            if ran.returncode == 0 or REFUSAL not in output:
                print(f"FAIL with {label} the player started (exit {ran.returncode}):\n{output[-1500:]}")
                failures += 1
            elif beside:
                print(f"FAIL with {label} the player refused but created {beside} beside itself")
                failures += 1
            else:
                print(f"ok with {label} the player refused and created nothing")

        ran = ask_features(player, str(data))
        beside = sorted(entry.name for entry in folder.iterdir() if entry != player)
        if ran.returncode != 0 or REFUSAL in ran.stdout + ran.stderr:
            print(f"FAIL with an absolute data folder the player did not start (exit {ran.returncode}):\n"
                  f"{(ran.stdout + ran.stderr)[-1500:]}")
            failures += 1
        elif beside:
            # A game's folders are in its data folder, and there is nothing of
            # RetroArch's usual layout beside the program.
            print(f"FAIL with an absolute data folder the player created {beside} beside itself")
            failures += 1
        else:
            made = sorted(entry.name for entry in data.iterdir())
            print(f"ok with an absolute data folder the player starts, and made {made} there and nothing beside itself")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
