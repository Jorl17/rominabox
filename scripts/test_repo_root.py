"""Check that tests and scripts use places in this repository and no others.

No test or script may use a place in one person's home, where that person
keeps their games. Every script that builds against RmlUi must use the
declared RmlUi, the picture tests must render with the helper in the builder
package, and no script may contain the removed experiment tree.

    uv run python scripts/test_repo_root.py

What this does NOT prove: that a place we accept contains what a test uses.
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
TESTS_AND_SCRIPTS = ["scripts", "desktop/crates", "desktop/src"]
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
    """Check, while they run, that the picture tests render with the helper in
    the builder package.

    A binary from a directory that we never create must not take precedence
    over the staged helper.
    """
    import menu_states
    import native_build
    import test_menu_preview
    from core_source import host_target

    packaged = native_build.preview_resource(host_target())
    for name, used in (("menu_states", menu_states.PREVIEW), ("test_menu_preview", test_menu_preview.RENDERER)):
        if used != packaged:
            return f"{name} renders with {used}, not the helper the builder packages, {packaged}"
    return None


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

    print("no test or script reaches a place outside this repository's own")
    return 0


if __name__ == "__main__":
    sys.exit(main())
