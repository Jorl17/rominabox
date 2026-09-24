"""Checks for the headless menu build: what we compile, and what we reuse.

A stale object in the cache is worse than no cache, because the tests then
pass against code that is no longer there. So we test each rule with small C
files made here, in a temporary directory, and never with the menu sources.

    python3 scripts/native_runtime/test_menu_harness.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import menu_harness  # noqa: E402
from scratch import scratch  # noqa: E402

FAILURES: list[str] = []


def check(condition: bool, message: str) -> None:
    if condition:
        print(f"  ok   {message}")
    else:
        print(f"  FAIL {message}")
        FAILURES.append(message)


def refused(call, *words: str) -> bool:
    """Whether `call` stops the build with every word in the message."""
    try:
        call()
    except SystemExit as stop:
        return all(word in str(stop) for word in words)
    return False


def compiling_reuses_only_what_is_unchanged() -> None:
    with scratch("rominabox-menu-harness-") as directory:
        base = Path(directory)
        (base / "shared.h").write_text("#define VALUE 1\n")
        (base / "other.h").write_text("#define OTHER 2\n")
        (base / "a.c").write_text('#include "shared.h"\nint a(void) { return VALUE; }\n')
        (base / "b.c").write_text('#include "other.h"\nint b(void) { return OTHER; }\n')
        (base / "main.c").write_text("int a(void); int b(void);\nint main(void) { return a() * 10 + b(); }\n")
        sources = [base / "a.c", base / "b.c", base / "main.c"]
        objects_dir = base / "objects"
        toolchain = menu_harness.Toolchain("cc", "c++", ("-I", directory), ())
        identity = toolchain.identity()

        def compile_with(chain=toolchain):
            return menu_harness.compile_objects(sources, chain, objects_dir, identity)

        def run(program: Path) -> int:
            return subprocess.run([str(program)]).returncode

        objects, compiled = compile_with()
        check(compiled == sources, "an empty cache compiles every source")
        program = base / "program"
        check(menu_harness.link(program, objects, toolchain, []), "the first link links")
        check(run(program) == 12, "the program is built from those sources")

        check(compile_with()[1] == [], "nothing changed: nothing is compiled")
        check(not menu_harness.link(program, objects, toolchain, []), "nothing changed: the link is reused")

        (base / "shared.h").write_text("#define VALUE 3\n")
        check(compile_with()[1] == [base / "a.c"], "a changed header rebuilds only the source that includes it")
        check(menu_harness.link(program, objects, toolchain, []), "a rebuilt object relinks")
        check(run(program) == 32, "the relinked program has the header's new value")

        (base / "b.c").write_text('#include "other.h"\nint b(void) { return OTHER + 1; }\n')
        check(compile_with()[1] == [base / "b.c"], "a changed source rebuilds only its own object")

        (objects_dir / menu_harness._object_name(base / "main.c")).unlink()
        check(compile_with()[1] == [base / "main.c"], "a missing object is rebuilt, alone")

        flagged = menu_harness.Toolchain("cc", "c++", ("-I", directory, "-DUNUSED=1"), ())
        check(compile_with(flagged)[1] == sources, "changed flags rebuild every object")
        check(compile_with()[1] == sources, "and changing them back rebuilds again")

        check(menu_harness.link(program, objects, toolchain, ["-lm"]), "a changed link flag relinks")
        check(run(program) == 33, "the relinked program is the current code")

        library = base / "libparts.a"
        check(menu_harness.archive(objects[:2], library), "the first archive is packed")
        check(not menu_harness.archive(objects[:2], library), "an unchanged archive is reused")
        (base / "a.c").write_text('#include "shared.h"\nint a(void) { return VALUE + 1; }\n')
        compile_with()
        check(menu_harness.archive(objects[:2], library), "a changed member repacks the archive")
        check(menu_harness.link(program, [objects[2], library], toolchain, []), "a program links against the archive")
        check(run(program) == 43, "and takes the archive's current members")


def the_block_is_read_as_the_player_build_reads_it() -> None:
    with scratch("rominabox-menu-harness-") as directory:
        base = Path(directory)
        for name in ("menu/one.cpp", "menu/two.c", "menu/mac.mm", "cheevos/stub.c"):
            (base / name).parent.mkdir(parents=True, exist_ok=True)
            (base / name).write_text("")
        makefile = base / "Makefile.common"
        block = (
            "OBJ += before.o\n"
            "ifeq ($(HAVE_RMLUI), 1)\n"
            "   OBJ += menu/one.o \\\n"
            "          menu/two.o\n"
            "   ifeq ($(HAVE_COCOA), 1)\n"
            "      OBJ += menu/mac.o\n"
            "   endif\n"
            "   ifneq ($(HAVE_CHEEVOS), 1)\n"
            "      OBJ += cheevos/stub.o\n"
            "   endif\n"
            "   DEFINES += -DHAVE_RMLUI\n"
            "endif\n"
            "OBJ += after.o\n"
        )
        makefile.write_text(block)

        def read(excluded: dict | None = None):
            return menu_harness.menu_sources(makefile, base, excluded or {})

        check(read() == [base / "menu/one.cpp", base / "menu/two.c", base / "cheevos/stub.c"],
              "the block's objects map to their sources, continuations and conditions included")
        check(read({"menu/two.o": "test"}) == [base / "menu/one.cpp", base / "cheevos/stub.c"],
              "an excluded object is left out")
        check(refused(lambda: read({"menu/gone.o": "test"}), "menu/gone.o"),
              "an exclusion the block no longer has stops the build")

        (base / "menu/one.cpp").unlink()
        check(refused(read, "menu/one.o"), "an object with no source stops the build")
        (base / "menu/one.c").write_text("")
        (base / "menu/one.cpp").write_text("")
        check(refused(read, "menu/one.o", "found 2"), "an object with two sources stops the build")
        (base / "menu/one.c").unlink()

        makefile.write_text(block.replace("HAVE_COCOA", "HAVE_WAYLAND"))
        check(refused(read, "HAVE_WAYLAND"), "a condition nobody has answered stops the build")
        makefile.write_text(block.replace("ifeq ($(HAVE_RMLUI), 1)", "ifeq ($(HAVE_RMLUI), 0)"))
        check(refused(read, "HAVE_RMLUI"), "a missing block stops the build")

    sources = menu_harness.menu_sources()
    check(bool(sources) and all(source.is_file() for source in sources),
          "the real Makefile.common block reads, and every source it names exists")


def depfiles_are_read_on_every_platform() -> None:
    posix = "objects/a.o: /src/a.c /src/with\\ space.h \\\n  /src/b.h\n"
    check(menu_harness._depfile_inputs(posix) == ["/src/a.c", "/src/with space.h", "/src/b.h"],
          "a POSIX depfile lists every input, including an escaped space")
    windows = "C:\\cache\\a.o: C:\\src\\a.c C:\\src\\a.h\n"
    check(menu_harness._depfile_inputs(windows) == ["C:\\src\\a.c", "C:\\src\\a.h"],
          "a Windows depfile is split after the target, not at the drive letter")


def main() -> int:
    for case in (compiling_reuses_only_what_is_unchanged, the_block_is_read_as_the_player_build_reads_it,
                 depfiles_are_read_on_every_platform):
        print(case.__name__)
        case()
    if FAILURES:
        print(f"\n{len(FAILURES)} menu harness check(s) failed")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
