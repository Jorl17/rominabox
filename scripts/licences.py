"""The licence text of every third-party component ROM-in-a-Box uses or
ships, in one place: licenses/ at the repository root, one file per component.

    uv run python scripts/licences.py                          # write what changed (reads the network for it)
    uv run python scripts/licences.py --refresh                # read every text again, network ones included
    uv run python scripts/licences.py --check                  # offline: warn of a missing, stale or unused entry
    uv run python scripts/licences.py --check --player-build DIR

Each entry contains the component, the version the repository uses, where it
comes from and what uses it, then each licence text under the place we read
it from. We take the texts from the components' own sources and never type
them here: Cargo's registry, node_modules, the fork at its checked-out commit,
the pinned archives, the font files' folders, and for the rest the URLs in
scripts/licences.json (we find them all in scripts/licence_sources.py).

The check uses no network. An entry with a text from this machine must be
exactly what we would write now, and one read from the network must contain
what the repository pins now. With --player-build we also warn about a
library of the fork, compiled in the player build, that has no entry. In
scripts/build_kit.py we copy the entries of the libraries a player uses into
its kit, which is in every exported game, and in scripts/prepare_runtime.py
the licence of the controller profiles.

We never remove or refuse anything because a licence text is missing. We
make a kit from every library its player uses, and name what has no text yet
in a warning, so that someone adds it here.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import licence_sources as sources  # noqa: E402
from licence_sources import Component, FetchError, Text  # noqa: E402

ROOT = sources.ROOT
OUT = ROOT / "licenses"
SECTION = "\n==> "
README = """\
The licence text of every third-party component ROM-in-a-Box uses or ships,
one file per component. scripts/licences.py writes this folder from each
component's own source; do not edit it by hand.

  native/      the player and what it links; a game carries those its player uses
  cores/       the libretro cores the builder downloads for an export
  crates/      the Rust crates the builder and its tools build with
  toolchains/  the Rust standard library the builder links
  npm/         the production packages of the builder's interface
  fonts/       the fonts of the menu designs and the builder
  data/        controller profiles, adapted artwork and catalogues

Each file names the component, the version the repository uses, where it
comes from, the licence it declares and what uses it, then each licence text,
headed by where it was read.

  uv run python scripts/licences.py            # write this folder again
  uv run python scripts/licences.py --check    # warn of a missing, stale or unused entry
"""


def entry_path(component: Component) -> Path:
    """Where in licenses/ a component's entry is."""
    return Path(component.group) / f"{component.name}.txt"


def entry(group: str, name: str) -> Path:
    """The entry of the component `name` of `group`."""
    return OUT / group / f"{name}.txt"


def clean(body: str) -> str:
    """A text as we store it in an entry: LF line ends, no trailing spaces, no
    blank lines around it."""
    lines = [line.rstrip() for line in body.replace("\r\n", "\n").replace("\r", "\n").split("\n")]
    return "\n".join(lines).strip("\n")


def header(component: Component) -> str:
    fields = [("Version", component.version), ("Source", component.source), ("Licence", component.licence),
              ("Used by", component.used_by), ("Note", component.note)]
    return "\n".join([component.title, "", *(f"{label + ':':<9} {value}" for label, value in fields if value)]) + "\n"


def render(component: Component, texts: list[Text]) -> str:
    return header(component) + "".join(f"{SECTION}{text.origin} <==\n\n{clean(text.body)}\n" for text in texts)


def sections(path: Path) -> dict[str, str]:
    """An entry's texts, by where each was read."""
    found = {}
    for section in path.read_text(encoding="utf-8").split(SECTION)[1:]:
        origin, _, body = section.partition(" <==\n\n")
        found[origin] = body.rstrip("\n")
    return found


def check_entries(folder: Path, components: list[Component]) -> list[str]:
    """What is wrong with the entries of `components` in `folder`."""
    problems = []
    for component in components:
        relative = entry_path(component)
        path = folder / relative
        if not path.is_file():
            problems.append(f"{relative.as_posix()}: no entry for {component.title} {component.version}, "
                            f"used by {component.used_by}")
            continue
        if component.local:
            try:
                wanted = render(component, component.read())
            except FetchError as error:
                problems.append(f"{relative.as_posix()}: {error}")
                continue
            if path.read_text(encoding="utf-8") != wanted:
                problems.append(f"{relative.as_posix()}: differs from its source; run uv run python scripts/licences.py")
            continue
        if not current(path, component):
            problems.append(f"{relative.as_posix()}: names another version or source than the repository pins, "
                            "or holds no text; run uv run python scripts/licences.py")
    return problems


def check(folder: Path = OUT, components: list[Component] | None = None) -> list[str]:
    """Everything wrong with `folder` for `components`, by default every
    component the repository uses now."""
    components = sources.discover() if components is None else components
    problems = []
    seen: dict[Path, Component] = {}
    for component in components:
        relative = entry_path(component)
        if relative in seen:
            problems.append(f"{relative.as_posix()}: names both {seen[relative].title} and {component.title}")
        seen[relative] = component
    problems += check_entries(folder, components)
    if not (folder / "README.txt").is_file() or (folder / "README.txt").read_text(encoding="utf-8") != README:
        problems.append("README.txt: differs from what scripts/licences.py writes")
    for path in sorted(folder.rglob("*")):
        relative = path.relative_to(folder)
        if path.is_file() and relative not in seen and relative != Path("README.txt"):
            problems.append(f"{relative.as_posix()}: no component the repository uses; run uv run python scripts/licences.py")
    for component in native_components():
        path = component.declared.get("path")
        if path and not sources.fork_has(path):
            problems.append(f"native/{component.name}.txt: the fork has no {path}")
    return problems


def current(path: Path, component: Component) -> bool:
    """Whether a written entry already contains what the repository pins and a
    text, in which case we do not read a text from the network again for it."""
    if not path.is_file():
        return False
    head, separator, texts = path.read_text(encoding="utf-8").partition(SECTION)
    return head == header(component) and bool(separator) and bool(texts.partition(" <==\n\n")[2].strip())


def generate(folder: Path = OUT, refresh: bool = False) -> list[str]:
    """Write every entry, and remove those that no component uses. We read a
    text from the network again only for an entry that is missing or has
    another version, or with `refresh`, because the branch we read a core's
    licence from can move. When we cannot read a text now, we keep its entry
    as it was and print that."""
    failures = []
    components = sources.discover()
    for component in components:
        relative = entry_path(component)
        if not component.local and not refresh and current(folder / relative, component):
            continue
        try:
            texts = component.read()
        except FetchError as error:
            failures.append(f"{relative.as_posix()}: {error}")
            continue
        if not texts:
            failures.append(f"{relative.as_posix()}: found no licence text for {component.title} {component.version}")
            continue
        (folder / relative).parent.mkdir(parents=True, exist_ok=True)
        (folder / relative).write_text(render(component, texts), encoding="utf-8", newline="\n")
    (folder / "README.txt").write_text(README, encoding="utf-8", newline="\n")
    wanted = {entry_path(component) for component in components} | {Path("README.txt")}
    for path in sorted(folder.rglob("*.txt")):
        if path.is_file() and path.relative_to(folder) not in wanted and folder in path.parents:
            path.unlink()
    counts: dict[str, int] = {}
    for component in components:
        counts[component.group] = counts.get(component.group, 0) + 1
    print("Wrote " + ", ".join(f"{count} {group}" for group, count in counts.items()) + f" entries in {folder}")
    return failures


# What a player build uses ----------------------------------------------------

def native_components() -> list[Component]:
    return sources.declared_components("native", sources.declared()["native"])


def compiled_fork_files(build: Path) -> set[str]:
    """The fork's files compiled or included in a player build, read from the
    dependency files written beside every object (-MMD)."""
    files: set[str] = set()
    roots: dict[Path, Path | None] = {}
    for dependencies in build.rglob("*.d"):
        folder = dependencies.parent
        if folder not in roots:
            roots[folder] = next((parent for parent in [folder, *folder.parents]
                                  if (parent / "Makefile.common").is_file() and build in [parent, *parent.parents]),
                                 None)
        root = roots[folder]
        if root is None:
            continue
        text = dependencies.read_text(encoding="utf-8", errors="replace").replace("\\\n", " ")
        for token in text.split():
            if token.endswith(":") or token.startswith("/") or os.path.isabs(token):
                continue
            normal = os.path.normpath(token).replace("\\", "/")
            if not normal.startswith("../"):
                files.add(normal)
    return files


def covers(component: Component, file: str) -> bool:
    path = component.declared.get("path")
    return path is not None and (path == "" or file.startswith(path + "/"))


def uncovered(files: set[str], components: list[Component], build: Path) -> list[str]:
    folders = sorted({"/".join(file.split("/")[:2]) for file in files if file.startswith("deps/")})
    return [f"{build} compiles the fork's {folder}, which no native component in scripts/licences.json names"
            for folder in folders
            if not any(covers(component, folder + "/") for component in components if component.declared.get("path"))]


def player_components(build: Path, platform: str) -> tuple[list[Component], list[str]]:
    """The native components of a player build with entries in licenses/
    (every player's, the fork libraries compiled in it, and its platform's
    runtime), and what is missing: a compiled fork library with no component,
    or an entry that is missing or out of date. We print what is missing and
    never refuse."""
    components = native_components()
    files = compiled_fork_files(build)
    missing = []
    if not files:
        missing.append(f"{build} holds no dependency files from the fork's build, "
                       "so the fork libraries its player uses cannot be read")
    missing += uncovered(files, components, build)
    used = []
    for component in components:
        declared = component.declared
        if "platforms" in declared:
            wanted = platform in declared["platforms"]
        else:
            wanted = declared.get("always") or any(covers(component, file) for file in files)
        if wanted:
            used.append(component)
    missing += check_entries(OUT, used)
    return [component for component in used if (OUT / entry_path(component)).is_file()], missing


def toolchain_differences(components: list[Component], installation: Path | None) -> list[str]:
    """Each text installed with the toolchain that differs from the entry we
    copy into the kit, which means that the pin in scripts/licences.json
    does not match the toolchain."""
    if installation is None:
        return []
    differences = []
    for component in components:
        texts = component.declared.get("texts", [])
        if not any("toolchain" in spec for spec in texts):
            continue
        written = set(sections(OUT / entry_path(component)).values())
        for spec in texts:
            if "toolchain" in spec:
                installed = installation / spec["toolchain"]
                if not installed.is_file() or clean(sources.decode(installed.read_bytes())) not in written:
                    differences.append(f"{installed} is not the text of licenses/{entry_path(component).as_posix()}: "
                                       "pin the toolchain's version in scripts/licences.json and run scripts/licences.py")
    return differences


def warning(missing: list[str]) -> str:
    """What we print in a kit build, a preparation or a check about licence
    texts that are missing or changed."""
    return ("WARNING: licence texts are missing or changed. Nothing was left out or refused; "
            "everything they belong to is used all the same. Bring licenses/ up to date "
            "(scripts/licences.json, then uv run python scripts/licences.py) so the games carry them:\n  "
            + "\n  ".join(missing))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="read no network; warn of any entry out of step")
    parser.add_argument("--player-build", type=Path, help="with --check: warn about a library of this player build no entry names")
    parser.add_argument("--refresh", action="store_true", help="read every text again, those on the network too")
    arguments = parser.parse_args()
    if arguments.check:
        # Licences are for attribution, so we print what is out of date and
        # nothing fails because of it.
        problems = check(OUT)
        if arguments.player_build is not None:
            problems += uncovered(compiled_fork_files(arguments.player_build), native_components(), arguments.player_build)
        print(warning(problems) if problems else f"Every component has its licence in {OUT}")
        return 0
    failures = generate(refresh=arguments.refresh)
    for failure in failures:
        print(failure)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
