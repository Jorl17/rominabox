"""Check that no test finds the repository by the path it was compiled in.

Every worktree shares one cargo target directory, because a target per
checkout costs several gigabytes. Cargo keys a build on the source
fingerprint, and two checkouts at the same commit have the same sources but
different paths. So one checkout can receive a test binary built in another,
and `env!("CARGO_MANIFEST_DIR")` in it still points at the checkout where it
was compiled.

The tests then read the files of another repository while reporting on this
one. That never produces an error, only a plausible wrong answer, for example:

  - the catalog tests fail with parity errors about a field these packages
    do not contain, because the binary came from a checkout whose packages do,
  - or the catalog tests pass while reading the files of another worktree.

`desktop/src-tauri/src/repo.rs` contains the rule for the desktop crate. The
catalog crate is standalone and cannot import from the desktop crate, so it
has a second copy of the rule. Here we check that both agree, and that any
new crate follows the rule too.

    python3 scripts/test_repo_root.py

What this does NOT prove: that the rule is right, or that a binary really was
built elsewhere. It proves that every place that resolves the repository
reads the environment first.
"""

from __future__ import annotations

import ast
import io
import re
import subprocess
import sys
import tokenize
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Places where a compiled-in path would lead to the repository. Files of the
# crate itself, such as an include_str! of a file beside the source, are a
# different case that we do not check here.
SEARCHED = [
    Path("desktop/src-tauri/src"),
    Path("desktop/src-tauri/tests"),
    Path("desktop/crates"),
]

MANIFEST = 'env!("CARGO_MANIFEST_DIR")'
# We look for a read of the variable, not a mention. A doc comment about the
# rule also contains the name, so a plain search would find it even with the
# compiled path in the code below.
OVERRIDE = re.compile(r'env::var\(\s*"ROMINABOX_REPO"\s*\)')

# To resolve the repository root, a function has to leave the crate. A path
# inside the crate, such as join("resources") or join("src"), points into the
# package itself, and we do not check it.
CLIMBS = re.compile(r'env!\("CARGO_MANIFEST_DIR"\)\s*\)?\s*\.?\s*\n?\s*\.join\("\.\.')

# A test that anyone can run must not use a folder in one person's home,
# where that person keeps their games. We look for any place in a home
# folder: /Users/<name>/, /home/<name>/,
# C:\\Users\\<name>, or ~/, $HOME/ and Path.home() anywhere but Library,
# where we keep the app's storage.
HOME_PLACE = re.compile(
    r"/Users/(?!Shared/)[^/\s\"'`]+/"
    r"|/home/[^/\s\"'`]+/"
    r"|\b[A-Za-z]:[\\/]+Users[\\/]+[^\\/\s\"'`]+"
    r"|(?:~|\$\{?HOME\}?)/(?!Library\b)[A-Za-z]"
    r"|Path\.home\(\)\s*/\s*[\"'](?!Library\b)"
)
# Tests and the scripts that run them, as they are in git. The source of a
# crate is not a test, and a patch is a record that we keep.
TESTS_AND_SCRIPTS = ["scripts", "desktop/src-tauri/tests", "desktop/crates", "desktop/src"]
COMMENT_STARTS = ("//", "#", "/*", "*", "--", "<!--")

# bridge, dcmenu and menu must not contain a directory that is not part of
# a checkout, because we never create it. Split so that this file does not
# contain the path that we forbid.
EXPERIMENT_TREE = "work/" + "experiments"
SCRIPT_SUFFIXES = {".py", ".sh", ".mjs", ".cpp", ".mm", ".h", ".c"}


def is_test_or_script(relative: str) -> bool:
    if relative.endswith(".patch"):
        return False
    if relative.startswith("desktop/crates/"):
        return "/tests/" in relative
    if relative.startswith("desktop/src/"):
        return ".test." in relative
    return True


def prose_lines(path: Path, text: str) -> set[int]:
    """Return the comment and docstring lines, which describe places and do not use them."""
    skipped: set[int] = set()
    if path.suffix != ".py":
        for number, line in enumerate(text.splitlines(), 1):
            if line.strip().startswith(COMMENT_STARTS):
                skipped.add(number)
        return skipped
    try:
        tree = ast.parse(text)
    except SyntaxError:
        return skipped
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)) and node.body:
            first = node.body[0]
            if (isinstance(first, ast.Expr) and isinstance(first.value, ast.Constant)
                    and isinstance(first.value.value, str)):
                skipped.update(range(first.lineno, first.end_lineno + 1))
    for token in tokenize.generate_tokens(io.StringIO(text).readline):
        if token.type == tokenize.COMMENT:
            skipped.add(token.start[0])
    return skipped


def home_places() -> list[str]:
    """Return the tests and scripts that use a place in one person's home."""
    tracked = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", *TESTS_AND_SCRIPTS],
        capture_output=True, text=True, check=True,
    ).stdout.split("\0")
    found: list[str] = []
    for relative in sorted(filter(None, tracked)):
        path = ROOT / relative
        # This file contains the patterns that we look for.
        if path.resolve() == Path(__file__).resolve():
            continue
        if not is_test_or_script(relative) or not path.is_file() or path.stat().st_size > 1_000_000:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        if not HOME_PLACE.search(text):
            continue
        prose = prose_lines(path, text)
        for number, line in enumerate(text.splitlines(), 1):
            if number not in prose and HOME_PLACE.search(line):
                found.append(f"{relative}:{number}")
    return found


def experiment_tree_mentions() -> list[str]:
    """Return the scripts and tests that still contain the removed experiment tree."""
    found: list[str] = []
    directory = ROOT / "scripts"
    for path in sorted(directory.rglob("*")):
        if not path.is_file() or path.suffix not in SCRIPT_SUFFIXES:
            continue
        if "__pycache__" in path.parts or path.stat().st_size > 1_000_000:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        if EXPERIMENT_TREE not in text:
            continue
        for number, line in enumerate(text.splitlines(), 1):
            if EXPERIMENT_TREE in line:
                found.append(f"{path.relative_to(ROOT)}:{number}")
    return found


def declaration_readers() -> list[str]:
    """Check that every script that compiles or links RmlUi uses the declared paths.

    We run each script and read what it compiles with and links. A copy of a
    path written into any of them appears as a value that differs from the
    declaration.
    """
    import rmlui_paths
    import prepare_rmlui
    import menu_interaction
    import test_dcmenu
    sys.path.insert(0, str(ROOT / "scripts/native_runtime"))
    import menu_harness

    drifted: list[str] = []
    for module in (menu_interaction, test_dcmenu):
        if module.LIBRARY != rmlui_paths.LIBRARY or list(module.HEADER_DIRS) != list(rmlui_paths.HEADER_DIRS):
            drifted.append(f"{module.__name__} links or includes RmlUi from somewhere else")
    if prepare_rmlui.LIBRARY != rmlui_paths.LIBRARY or prepare_rmlui.HEADER != rmlui_paths.HEADER:
        drifted.append("prepare_rmlui builds RmlUi somewhere the tests do not look")
    compiled = menu_harness.headless([]).cxxflags
    if any(f"-I{path}" not in compiled for path in rmlui_paths.HEADER_DIRS):
        drifted.append("menu_harness compiles the menu against other RmlUi headers")
    if menu_harness.rmlui_paths.LIBRARY != rmlui_paths.LIBRARY:
        drifted.append("menu_harness links another RmlUi")
    return drifted


def staged_preview() -> str | None:
    """Check that we render menu_states with the helper in the builder package.

    We use the staged helper, not a binary from a directory that we never
    create.
    """
    import test_menu_preview

    relative = test_menu_preview.RENDERER.relative_to(ROOT).as_posix()
    text = (ROOT / "scripts/menu_states.py").read_text(encoding="utf-8")
    if relative not in text:
        return "menu_states.py does not use the staged preview helper"
    return None


def climbing_uses(path: Path) -> list[int]:
    """Return the line numbers where a compiled-in path leads out of the crate."""
    text = path.read_text(errors="replace")
    found: list[int] = []
    for match in CLIMBS.finditer(text):
        found.append(text.count("\n", 0, match.start()) + 1)
    return found


def main() -> int:
    named_tree = experiment_tree_mentions()
    if named_tree:
        print(
            f"{len(named_tree)} script(s) or test(s) name the removed experiment tree:",
            file=sys.stderr,
        )
        for hit in named_tree:
            print(f"  FAIL {hit}", file=sys.stderr)
        return 1

    drifted = declaration_readers()
    preview = staged_preview()
    if preview:
        drifted.append(preview)
    if drifted:
        print("the RmlUi declaration is not what the tests read:", file=sys.stderr)
        for hit in drifted:
            print(f"  FAIL {hit}", file=sys.stderr)
        return 1

    named = home_places()
    if named:
        print(
            f"{len(named)} place(s) in a test or script are in one person's home, where "
            "their games are; use generated or fetched content, or a path in no one's home:",
            file=sys.stderr,
        )
        for hit in named:
            print(f"  FAIL {hit}", file=sys.stderr)
        return 1

    offenders: list[str] = []
    checked = 0
    exempt = 0

    for relative in SEARCHED:
        directory = ROOT / relative
        if not directory.is_dir():
            continue
        for path in sorted(directory.rglob("*.rs")):
            if "target" in path.parts:
                continue
            lines = climbing_uses(path)
            if not lines:
                continue
            checked += 1
            text = path.read_text(errors="replace")
            if OVERRIDE.search(text):
                exempt += 1
                print(f"  ok   {path.relative_to(ROOT)}")
                continue
            where = ", ".join(str(line) for line in lines)
            print(f"  FAIL {path.relative_to(ROOT)}:{where}", file=sys.stderr)
            offenders.append(str(path.relative_to(ROOT)))

    if not checked:
        raise SystemExit(
            "no file climbs out of its crate with a compiled-in path, which "
            "means this check has stopped matching how the code is written "
            "and is proving nothing"
        )

    if offenders:
        print(
            f"\n{len(offenders)} file(s) reach the repository by the path they "
            f"were compiled in: {', '.join(offenders)}.\n"
            "A worktree shares one cargo target, so that path is whichever "
            "checkout built last. Ask the environment first:\n"
            '    match std::env::var("ROMINABOX_REPO") {\n'
            "        Ok(declared) if !declared.is_empty() => PathBuf::from(declared),\n"
            '        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."),\n'
            "    }",
            file=sys.stderr,
        )
        return 1

    print(f"\nall {exempt} place(s) that resolve the repository ask the environment first")
    return 0


if __name__ == "__main__":
    sys.exit(main())
