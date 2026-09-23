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

import re
import sys
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

# Split so that this file does not contain the directory name that we forbid.
# A test that anyone can run must not use a folder in one person's home.
COLLECTION = "roms-" + "to-test"
SCANNED = [Path("desktop"), Path("scripts")]
SKIPPED_PARTS = {"target", "node_modules", "dist", "work"}


def collection_mentions() -> list[str]:
    """Return the tests and scripts that contain one person's ROM directory."""
    found: list[str] = []
    for relative in SCANNED:
        directory = ROOT / relative
        if not directory.is_dir():
            continue
        for path in sorted(directory.rglob("*")):
            if not path.is_file() or SKIPPED_PARTS.intersection(path.parts):
                continue
            if path.stat().st_size > 1_000_000:
                continue
            try:
                text = path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue
            if COLLECTION not in text:
                continue
            for number, line in enumerate(text.splitlines(), 1):
                if COLLECTION in line:
                    found.append(f"{path.relative_to(ROOT)}:{number}")
    return found


def climbing_uses(path: Path) -> list[int]:
    """Return the line numbers where a compiled-in path leads out of the crate."""
    text = path.read_text(errors="replace")
    found: list[int] = []
    for match in CLIMBS.finditer(text):
        found.append(text.count("\n", 0, match.start()) + 1)
    return found


def main() -> int:
    named = collection_mentions()
    if named:
        print(
            f"{len(named)} test(s) or script(s) name one person's ROM directory:",
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
