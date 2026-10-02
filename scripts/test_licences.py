"""Check that licenses/ contains the licence of every third-party component,
and that the check fails when one is missing.

    uv run python scripts/test_licences.py

The folder in the repository must pass `scripts/licences.py --check`. We
show the failures on a copy of it in a temporary directory: an entry
removed, a text changed, a file that no component uses, an entry for another
version. For a made-up player build with a fork library that has no entry,
we still put every library in the kit and warn with that library's name.
We read nothing from the network and do not change licenses/.
"""

from __future__ import annotations

import shutil
import sys
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import licences  # noqa: E402
import scratch  # noqa: E402

FAILURES: list[str] = []


def check(condition: bool, message: str, detail: str = "") -> None:
    if condition:
        print(f"  ok   {message}")
    else:
        print(f"  FAIL {message}\n{detail}")
        FAILURES.append(message)


def names(problems: list[str], *words: str) -> bool:
    return any(all(word in problem for word in words) for problem in problems)


def fake_build(root: Path, compiled: list[str]) -> Path:
    """Make a player build folder as the fork's makefile leaves it, with
    dependency files beside the objects that list the sources of each object."""
    retroarch = root / "build" / "macos-arm64" / "retroarch"
    (retroarch / "obj-unix/release/deps").mkdir(parents=True)
    (retroarch / "Makefile.common").write_text("")
    (retroarch / "obj-unix/release/deps/player.d").write_text(
        "obj-unix/release/deps/player.o: " + " \\\n  ".join(compiled) + "\n")
    # We compile RmlUi and FreeType outside the fork, so they are not part of it.
    (root / "build" / "macos-arm64" / "preview").mkdir()
    (root / "build" / "macos-arm64" / "preview" / "preview.d").write_text("preview.o: ../vendor/RmlUi/x.cpp\n")
    return root / "build"


def main() -> int:
    components = licences.sources.discover()
    problems = licences.check(components=components)
    check(not problems, "every component the repository uses has a current entry in licenses/", "\n".join(problems))
    groups = {component.group for component in components}
    check(groups == {"native", "cores", "crates", "toolchains", "npm", "fonts", "data"},
          "the components come from every source", str(groups))

    with scratch.scratch("rominabox-licences-") as temporary:
        folder = Path(temporary) / "licenses"
        shutil.copytree(licences.OUT, folder)
        # Every way in which an entry can be wrong, all in one check of the copy.
        (folder / "native/glslang.txt").unlink()
        crate = next(component for component in components if component.group == "crates" and component.local)
        entry = folder / licences.entry_path(crate)
        entry.write_text(entry.read_text(encoding="utf-8") + "A line its source does not have.\n", encoding="utf-8")
        core = next(component for component in components if component.group == "cores")
        entry = folder / licences.entry_path(core)
        entry.write_text(entry.read_text(encoding="utf-8").replace(core.version, "an older build"), encoding="utf-8")
        (folder / "crates/left-behind-0.1.0.txt").write_text("MIT\n", encoding="utf-8")
        problems = licences.check(folder, components=components)
        shown = "\n".join(problems)
        check(names(problems, "native/glslang.txt", "no entry"), "a component with no entry fails the check", shown)
        check(names(problems, licences.entry_path(crate).as_posix(), "differs from its source"),
              "an entry whose text is not its source's fails the check", shown)
        check(names(problems, licences.entry_path(core).as_posix(), "another version"),
              "an entry read from the network that names another version fails the check", shown)
        check(names(problems, "crates/left-behind-0.1.0.txt", "no component"),
              "an entry no component uses fails the check", shown)
        check(len(problems) == 4, "and nothing else does", shown)
        # Licences are for attribution, so we only warn about these problems
        # and fail nothing.
        with patch.object(licences, "OUT", folder), patch.object(sys, "argv", ["licences.py", "--check"]):
            status = licences.main()
        check(status == 0, "a check that finds them fails nothing", f"exit status {status}")

        # We warn about a library with no entry and still keep it in the kit.
        unheard = fake_build(Path(temporary) / "unheard", ["deps/mbedtls/aes.c", "deps/unheard-of/lib.c"])
        used, missing = licences.player_components(unheard, "macos")
        check("mbedtls" in [component.name for component in used],
              "a kit is made from a player build compiling a fork library with no entry", str(used))
        check(names(missing, "deps/unheard-of", "no native component") and len(missing) == 1,
              "and the fork library with no entry is named in its warning", "\n".join(missing))
        warning = licences.warning(missing)
        check("deps/unheard-of" in warning and "Nothing was left out or refused" in warning,
              "the warning says nothing was left out", warning)

        build = fake_build(Path(temporary) / "known", ["deps/mbedtls/aes.c", "gfx/../deps/yxml/yxml.h", "retroarch.c"])
        used, missing = licences.player_components(build, "macos")
        used = [component.name for component in used]
        check("mbedtls" in used and "yxml" in used and "ibxm" not in used and "mingw-w64-runtime" not in used,
              "a player build uses the fork libraries it compiled, not the others", str(used))
        check({"retroarch", "libretro-common", "rmlui", "freetype"} <= set(used),
              "a player build uses what every player is made from", str(used))
        check(not missing, "a player build whose libraries all have entries warns of nothing", "\n".join(missing))
        windows = [component.name for component in licences.player_components(build, "windows")[0]]
        check("mingw-w64-runtime" in windows, "a Windows player build uses its runtime's licences", str(windows))

    guarded = "#ifndef LIB_H\n#define LIB_H\n\n/* Copyright the authors. Permission is granted. */\n#endif\n"
    comment = licences.sources.first_comment(guarded)
    check(comment == "Copyright the authors. Permission is granted.",
          "a header's first comment is read past its include guard", repr(comment))
    try:
        licences.sources.first_comment("#ifndef LIB_H\n#define OTHER_H\n/* Not after a guard. */\n")
        check(False, "a comment after lines that are not an include guard is not the first comment")
    except licences.sources.FetchError:
        check(True, "a comment after lines that are not an include guard is not the first comment")

    if FAILURES:
        print(f"\n{len(FAILURES)} licence case(s) failed")
        return 1
    print("\nlicences: every case holds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
