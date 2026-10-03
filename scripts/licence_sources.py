"""Where each third-party component's licence text comes from, for
scripts/licences.py.

With each function here we list the components of one kind without the
network: what each component is, which version the repository uses, and how
to read its text. We read a text from the network only at the moment we use
it, so `licences.py --check` can list every component offline.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import tarfile
import tomllib
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath
from typing import Callable

from built import WORKSPACE as BUILDER_WORKSPACE

ROOT = Path(__file__).resolve().parent.parent
DECLARED = ROOT / "scripts/licences.json"
FORK = ROOT / "vendor/retroarch"
FORK_SOURCE = "https://github.com/Jorl17/rominabox-retroarch"
CORE_PINS = ROOT / "desktop/core-pins.json"
NPM_LOCK = ROOT / "desktop/package-lock.json"
# The builder's package, whose dependencies include the engine's and the
# command line's: they share the builder's Cargo workspace, BUILDER_WORKSPACE.
# The other crates in desktop/crates are workspaces of their own, each named
# for its package.
BUILDER_PACKAGE = BUILDER_WORKSPACE / "src-tauri"
# The targets the builder is compiled for: macOS on either processor, and
# Windows with the MSVC toolchain (.cargo/config.toml).
BUILDER_TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc")
FONT_SUFFIXES = (".ttf", ".otf", ".woff", ".woff2")

LICENCE_NAME = re.compile(r"^(licen[cs]e|copying|notice|unlicense|copyright)([-._].*)?$", re.IGNORECASE)
# Files named like a licence that are code or metadata, not a licence text.
NOT_TEXT = {".rs", ".py", ".c", ".h", ".cc", ".cpp", ".hpp", ".toml", ".json", ".sh", ".js", ".mjs",
            ".ts", ".yml", ".yaml", ".spdx", ".go", ".html", ".xml"}
# Folders of a package with test data, not what is in the published package.
NOT_SHIPPED = {"tests", "test", "testdata", "examples", "benches", "fuzz", "target", ".git"}


class FetchError(Exception):
    pass


@dataclass
class Text:
    origin: str
    body: str


@dataclass
class Component:
    group: str
    name: str
    title: str
    version: str
    source: str
    used_by: str
    read: Callable[[], list[Text]]
    # Whether `read` works without the network. We then compare the whole entry
    # in the check, not only what it contains about the component.
    local: bool
    licence: str = ""
    note: str = ""
    declared: dict = field(default_factory=dict)


def declared() -> dict:
    return json.loads(DECLARED.read_text(encoding="utf-8"))


def decode(data: bytes) -> str:
    try:
        return data.decode("utf-8-sig")
    except UnicodeDecodeError:
        return data.decode("latin-1")


def is_licence_file(path: PurePosixPath | Path) -> bool:
    """A licence by its name, or any text in a LICENSES folder (REUSE)."""
    if path.suffix.lower() in NOT_TEXT:
        return False
    return bool(LICENCE_NAME.match(path.name)) or path.parent.name.lower() in ("licenses", "licences")


def licence_folder(path: PurePosixPath) -> str:
    """The folder a licence file applies to: its own folder, or for a file in a
    LICENSES folder, the folder that contains that one."""
    parent = path.parent
    return str(parent.parent if parent.name.lower() in ("licenses", "licences") else parent)


def clarified_texts(where: dict, version: str) -> list[Text]:
    """The licence files in a folder of a GitHub repository, for a package
    published without any (scripts/licences.json, `clarified`)."""
    ref = where["ref"].format(version=version)
    listing = json.loads(fetch(f"https://api.github.com/repos/{where['github']}/contents/"
                               f"{where['directory']}?ref={urllib.parse.quote(ref, safe='')}"))
    return [fetch_text(item["download_url"]) for item in sorted(listing, key=lambda item: item["name"])
            if item["type"] == "file" and is_licence_file(PurePosixPath(item["path"]))]


_FETCHED: dict[str, bytes] = {}


def fetch(url: str) -> bytes:
    """The body of `url`, once per run. We use GITHUB_TOKEN for the GitHub API
    when it is set, because the limit without it is sixty requests an hour."""
    if url in _FETCHED:
        return _FETCHED[url]
    headers = {"User-Agent": "rominabox-licences"}
    if url.startswith("https://api.github.com/"):
        headers["Accept"] = "application/vnd.github+json"
        if os.environ.get("GITHUB_TOKEN"):
            headers["Authorization"] = f"Bearer {os.environ['GITHUB_TOKEN']}"
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=60) as response:
            _FETCHED[url] = response.read()
    except OSError as error:
        raise FetchError(f"{url}: {error}") from error
    return _FETCHED[url]


def fetch_text(url: str) -> Text:
    return Text(url, decode(fetch(url)))


def github_commit(repository: str, ref: str) -> str:
    """The current commit of the branch `ref` in `repository`, which we get from
    git instead of from the rate-limited API."""
    listed = subprocess.run(["git", "ls-remote", f"https://github.com/{repository}", f"refs/heads/{ref}"],
                            capture_output=True, text=True, encoding="utf-8")
    if listed.returncode != 0 or not listed.stdout.strip():
        raise FetchError(f"https://github.com/{repository} has no branch {ref}: {listed.stderr.strip()}")
    return listed.stdout.split()[0]


def branch_text(repository: str, ref: str, path: str) -> Text:
    """A file of a GitHub repository on the branch `ref` now, read at the
    branch's current commit, which we record in the origin of the text."""
    return fetch_text(f"https://raw.githubusercontent.com/{repository}/{github_commit(repository, ref)}/{path}")


def repository_texts(repository: str, commit: str, directory: str) -> list[Text]:
    """The licence files of `repository` at `commit`: those in `directory`,
    or in the nearest folder above it that has any."""
    match = re.match(r"https?://(github\.com|gitlab\.com)/([^/]+/[^/]+?)(?:\.git)?/?$", repository or "")
    if not match:
        return []
    host, project = match.groups()
    folders = [str(PurePosixPath(directory).parents[index]) for index in range(len(PurePosixPath(directory).parts))]
    candidates = [directory, *folders] if directory not in ("", ".") else ["."]
    if host == "github.com":
        tree = json.loads(fetch(f"https://api.github.com/repos/{project}/git/trees/{commit}?recursive=1"))
        files = [PurePosixPath(entry["path"]) for entry in tree["tree"] if entry["type"] == "blob"]
        for folder in candidates:
            found = sorted(str(path) for path in files if licence_folder(path) == folder and is_licence_file(path))
            if found:
                return [fetch_text(f"https://raw.githubusercontent.com/{project}/{commit}/{path}") for path in found]
        return []
    for folder in candidates:
        listing = json.loads(fetch(
            f"https://gitlab.com/api/v4/projects/{urllib.parse.quote(project, safe='')}/repository/tree"
            f"?ref={commit}&per_page=100" + ("" if folder == "." else f"&path={urllib.parse.quote(folder)}")))
        found = sorted(entry["path"] for entry in listing
                       if entry["type"] == "blob" and is_licence_file(PurePosixPath(entry["path"])))
        if not found:
            found = [f"{entry['path']}/{name}" for entry in listing if entry["type"] == "tree"
                     and entry["name"].lower() in ("licenses", "licences")
                     for name in gitlab_names(project, commit, entry["path"])]
        if found:
            return [fetch_text(f"https://gitlab.com/{project}/-/raw/{commit}/{path}") for path in found]
    return []


# The fork -------------------------------------------------------------------

def fork_commit() -> str:
    return subprocess.run(["git", "-C", str(FORK), "rev-parse", "HEAD"],
                          capture_output=True, text=True, encoding="utf-8", check=True).stdout.strip()


def fork_file(path: str) -> str:
    """A file of the fork as its checked-out commit has it."""
    shown = subprocess.run(["git", "-C", str(FORK), "show", f"HEAD:{path}"], capture_output=True)
    if shown.returncode != 0:
        raise FetchError(f"the fork has no {path} at {fork_commit()}")
    return decode(shown.stdout)


def fork_has(path: str) -> bool:
    return subprocess.run(["git", "-C", str(FORK), "cat-file", "-e", f"HEAD:{path}"],
                          capture_output=True).returncode == 0


def comment_lines(block: list[str]) -> str:
    lines = []
    for line in block:
        line = re.sub(r"^\s*/\*+!? ?", "", line)
        line = re.sub(r"\*+/\s*$", "", line)
        line = re.sub(r"^\s*(\*|//)(?!/) ?", "", line)
        lines.append(line.rstrip())
    return "\n".join(lines).strip("\n")


def first_comment(text: str) -> str:
    """The comment a file starts with, read past a header's include guard:
    the fork's copy of zlib.h opens with one."""
    lines = text.splitlines()
    start = next((index for index, line in enumerate(lines) if line.strip()), len(lines))
    guard = re.fullmatch(r"\s*#\s*ifndef\s+(\w+)\s*", lines[start]) if start < len(lines) else None
    if guard and start + 1 < len(lines) and re.fullmatch(rf"\s*#\s*define\s+{guard.group(1)}\s*", lines[start + 1]):
        start = next((index for index in range(start + 2, len(lines)) if lines[index].strip()), len(lines))
    if start < len(lines) and lines[start].lstrip().startswith("//"):
        end = start
        while end < len(lines) and lines[end].lstrip().startswith("//"):
            end += 1
        return comment_lines(lines[start:end])
    if start < len(lines) and lines[start].lstrip().startswith("/*"):
        end = next(index for index in range(start, len(lines)) if "*/" in lines[index])
        return comment_lines(lines[start:end + 1])
    raise FetchError("the file does not start with a comment")


def last_comments(text: str, count: int) -> str:
    """The last `count` block comments of a file, in their order."""
    blocks = []
    end = len(text)
    for _ in range(count):
        end = text.rfind("*/", 0, end)
        start = text.rfind("/*", 0, end)
        if start < 0 or end < 0:
            raise FetchError("the file has fewer block comments than declared")
        blocks.insert(0, comment_lines(text[start:end + 2].splitlines()))
        end = start
    return "\n\n".join(blocks)


def fork_define(path: str, macro: str) -> str:
    found = re.search(rf'#define\s+{macro}\s+"([^"]+)"', fork_file(path))
    if not found:
        raise FetchError(f"the fork's {path} defines no {macro}")
    return found.group(1)


# Pins ----------------------------------------------------------------------

@dataclass
class Pin:
    version: str
    source: str
    values: dict
    archive: Path | None = None
    download: Callable[[], Path] | None = None
    prefix: str = ""


def pin(name: str, spec: dict) -> Pin:
    """What a component's pin contains: its version, where it comes from, and
    for an archive, where the archive is and how to get it."""
    import native_build
    import prepare_runtime
    if name == "fork":
        path = spec.get("path", "")
        version = (fork_define(*spec["versionDefine"]) if "versionDefine" in spec
                   else "as vendored in the RetroArch fork")
        return Pin(version, FORK_SOURCE + (f" ({path})" if path else ""), {"version": version})
    if name == "rmlui":
        rmlui = native_build.recipe()["rmlui"]
        return Pin(rmlui["commit"], rmlui["repository"], {"commit": rmlui["commit"]})
    release = native_build.recipe().get(name)
    if isinstance(release, dict) and "url" in release:
        # A library we build in the player recipe from its pinned release.
        archive = native_build.DOWNLOADS / release["url"].rsplit("/", 1)[1]
        return Pin(release["directory"].removeprefix(f"{name}-"), release["url"], {}, archive,
                   lambda: native_build.download(release["url"], release["sha256"]), release["directory"])
    if name == "joypad":
        revision = prepare_runtime.JOYPAD_AUTOCONFIG_REVISION
        repository = prepare_runtime.JOYPAD_AUTOCONFIG_REPO
        archive = prepare_runtime.DOWNLOADS / f"{prepare_runtime.JOYPAD_AUTOCONFIG_COMPONENT}-{revision}.tar.gz"
        url = f"https://codeload.github.com/{repository}/tar.gz/{revision}"

        def download() -> Path:
            prepare_runtime.download(url, archive)
            return archive
        return Pin(revision, f"https://github.com/{repository}/tree/{revision}", {"revision": revision},
                   archive, download, f"{repository.split('/')[1]}-{revision}")
    pinned = declared()["pins"][name]
    return Pin(pinned["revision"], pinned["source"], {"revision": pinned["revision"]})


def spec_text(spec: dict, pinned: Pin) -> Text:
    if "fork" in spec:
        return Text(f"{spec['fork']} in the RetroArch fork", fork_file(spec["fork"]))
    if "forkComment" in spec:
        return Text(f"the first comment of {spec['forkComment']} in the RetroArch fork",
                    first_comment(fork_file(spec["forkComment"])))
    if "forkLastComment" in spec:
        count = spec.get("count", 1)
        return Text(f"the last {'comment' if count == 1 else f'{count} comments'} of {spec['forkLastComment']} "
                    "in the RetroArch fork", last_comments(fork_file(spec["forkLastComment"]), count))
    if "member" in spec:
        archive = pinned.archive if pinned.archive and pinned.archive.is_file() else pinned.download()
        member = f"{pinned.prefix}/{spec['member']}"
        with tarfile.open(archive) as package:
            extracted = package.extractfile(member)
            if extracted is None:
                raise FetchError(f"{archive.name} has no {member}")
            return Text(f"{member} in {pinned.source}", decode(extracted.read()))
    return fetch_text(spec["url"].format(**pinned.values))


def spec_is_local(spec: dict, pinned: Pin) -> bool:
    if any(key in spec for key in ("fork", "forkComment", "forkLastComment")):
        return True
    return "member" in spec and pinned.archive is not None and pinned.archive.is_file()


def declared_components(group: str, entries: list[dict]) -> list[Component]:
    """The components declared in scripts/licences.json under `group`."""
    found = []
    for entry in entries:
        if "pin" in entry:
            pinned = pin(entry["pin"], entry)
        elif "github" in entry:
            pinned = Pin(f"the {entry['ref']} branch, which is what the builder reads",
                         f"https://github.com/{entry['github']}", {})
        else:
            pinned = Pin(entry["version"], entry["source"], {"version": entry["version"]})
        texts = entry["texts"]

        def read(texts=texts, pinned=pinned, entry=entry) -> list[Text]:
            if "github" in entry:
                return [branch_text(entry["github"], entry["ref"], spec["githubFile"]) for spec in texts]
            return [spec_text(spec, pinned) for spec in texts]
        found.append(Component(
            group, entry["name"], entry["title"], pinned.version, pinned.source, entry["usedBy"], read,
            all(spec_is_local(spec, pinned) for spec in texts), entry.get("licence", ""), entry.get("note", ""),
            entry))
    return found


# Cores ---------------------------------------------------------------------

def cores() -> list[Component]:
    """Every core we can download in the builder, with the licence we download
    beside it: the text on the branch of the core's repository."""
    found = []
    for core in json.loads(CORE_PINS.read_text(encoding="utf-8"))["cores"]:
        repository, ref, path = core["repository"], core["licenseRef"], core["licensePath"]
        if not ref or not path:
            continue

        def read(repository=repository, ref=ref, path=path) -> list[Text]:
            return [branch_text(repository, ref, path)]
        found.append(Component(
            "cores", core["component"], core["component"], f"the buildbot's nightly build; licence from {ref}",
            f"https://github.com/{repository}",
            "games exported with this core: the builder downloads it, with this licence, when an export needs it",
            read, False))
    return found


# Crates --------------------------------------------------------------------

def builder_members() -> set[Path]:
    """The packages of the builder's Cargo workspace, which the builder's
    listing covers."""
    root = ROOT / BUILDER_WORKSPACE
    document = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    return {(root / member).resolve() for member in document["workspace"]["members"]}


def workspaces() -> dict[Path, str]:
    """The builder's package and every crate in desktop/crates outside the
    builder's workspace, and what each one is. Such a crate with no lockfile
    must have no dependencies, because we could not list them without
    writing one."""
    found = {ROOT / BUILDER_PACKAGE: "the builder"}
    members = builder_members()
    for manifest in sorted((ROOT / "desktop/crates").glob("*/Cargo.toml")):
        if manifest.parent.resolve() in members:
            continue
        document = tomllib.loads(manifest.read_text(encoding="utf-8"))
        if not (manifest.parent / "Cargo.lock").is_file():
            if any(key.endswith("dependencies") and key != "dev-dependencies" and value
                   for key, value in [*document.items(), *(item for target in document.get("target", {}).values()
                                                           for item in target.items())]):
                raise FetchError(f"{manifest.parent} has dependencies and no Cargo.lock to list them from")
            continue
        found[manifest.parent] = document["package"]["name"]
    return found


def cargo_packages() -> dict[tuple[str, str], tuple[dict, set[str]]]:
    """Every third-party package we build a workspace with, outside its
    development dependencies, and the workspaces that use it."""
    packages: dict[tuple[str, str], tuple[dict, set[str]]] = {}
    for workspace, user in workspaces().items():
        platforms = [argument for target in BUILDER_TARGETS for argument in ("--filter-platform", target)]
        metadata = json.loads(subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked", *platforms,
             "--manifest-path", str(workspace / "Cargo.toml")],
            capture_output=True, text=True, encoding="utf-8", check=True).stdout)
        by_id = {package["id"]: package for package in metadata["packages"]}
        nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
        seen: set[str] = set()
        waiting = [metadata["resolve"]["root"]]
        while waiting:
            identifier = waiting.pop()
            if identifier in seen:
                continue
            seen.add(identifier)
            for dependency in nodes[identifier]["deps"]:
                if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"]):
                    waiting.append(dependency["pkg"])
        for identifier in seen:
            package = by_id[identifier]
            if package["source"] is None:
                continue
            key = (package["name"], package["version"])
            packages.setdefault(key, (package, set()))[1].add(user)
    return packages


def package_files(directory: Path, declared_file: str | None = None) -> list[Path]:
    """The licence files in a package: at its root, in folders of its own code,
    and the one listed in its manifest."""
    found = {path for path in directory.rglob("*")
             if path.is_file() and is_licence_file(path)
             and not NOT_SHIPPED.intersection(path.relative_to(directory).parts[:-1])}
    if declared_file and (directory / declared_file).is_file():
        found.add(directory / declared_file)
    return sorted(found, key=lambda path: (len(path.relative_to(directory).parts), str(path)))


def gitlab_names(project: str, commit: str, folder: str) -> list[str]:
    listing = json.loads(fetch(f"https://gitlab.com/api/v4/projects/{urllib.parse.quote(project, safe='')}"
                               f"/repository/tree?ref={commit}&per_page=100&path={urllib.parse.quote(folder)}"))
    return sorted(entry["name"] for entry in listing if entry["type"] == "blob")


def crates() -> list[Component]:
    clarified = declared()["clarified"]["crates"]
    found = []
    for (name, version), (package, users) in sorted(cargo_packages().items()):
        directory = Path(package["manifest_path"]).parent
        files = package_files(directory, package.get("license_file"))
        # A licence in one of its folders belongs to a part (gilrs contains SDL's
        # controller database), not to the crate itself.
        own = any(licence_folder(PurePosixPath(path.relative_to(directory).as_posix())) == "." for path in files)
        repository = package.get("repository") or ""

        def read(files=files, own=own, directory=directory, repository=repository, name=name,
                 version=version) -> list[Text]:
            shipped = [Text(path.relative_to(directory).as_posix(), decode(path.read_bytes())) for path in files]
            if own:
                return shipped
            if name in clarified:
                return [*clarified_texts(clarified[name], version), *shipped]
            # Published without a licence file: the repository at the commit
            # the package was published from.
            vcs = directory / ".cargo_vcs_info.json"
            if not vcs.is_file():
                return []
            info = json.loads(vcs.read_text(encoding="utf-8"))
            found = repository_texts(repository, info["git"]["sha1"], info.get("path_in_vcs", ""))
            return [*found, *shipped] if found else []
        found.append(Component(
            "crates", f"{name}-{version}", name, version,
            f"crates.io: {name} {version}" + (f", {repository}" if repository else ""),
            ", ".join(sorted(users)), read, own, package.get("license") or "",
            clarified.get(name, {}).get("note", "")))
    return found


def rust_std() -> list[Component]:
    """The Rust standard library, which every Rust program links."""
    def read() -> list[Text]:
        sysroot = Path(subprocess.run(["rustc", "--print", "sysroot"], capture_output=True, text=True, encoding="utf-8",
                                      check=True).stdout.strip())
        rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True, encoding="utf-8", check=True).stdout.strip()
        documents = sysroot / "share/doc/rust"
        if not (documents / "COPYRIGHT-library.html").is_file():
            raise FetchError(f"{rustc} has no {documents / 'COPYRIGHT-library.html'} (rustup's rust-docs)")
        files = [documents / "COPYRIGHT-library.html", *sorted((documents / "licenses").glob("*.txt"))]
        return [Text(f"{path.relative_to(sysroot).as_posix()} of {rustc}", decode(path.read_bytes()))
                for path in files]
    return [Component("toolchains", "rust-std", "The Rust standard library",
                      "the Rust toolchain the builder is compiled with", "https://github.com/rust-lang/rust",
                      "the builder and its command line", read, False, "MIT OR Apache-2.0")]


# npm -----------------------------------------------------------------------

def npm() -> list[Component]:
    """The builder's production packages, as resolved in package-lock.json."""
    clarified = declared()["clarified"]["npm"]
    found = []
    for key, entry in sorted(json.loads(NPM_LOCK.read_text(encoding="utf-8"))["packages"].items()):
        if not key or entry.get("dev") or entry.get("devOptional"):
            continue
        name = entry.get("name") or key.rsplit("node_modules/", 1)[1]
        version = entry["version"]
        directory = NPM_LOCK.parent / key
        files = sorted(path for path in directory.iterdir() if path.is_file() and is_licence_file(path)) \
            if directory.is_dir() else []
        manifest = json.loads((directory / "package.json").read_text(encoding="utf-8")) \
            if (directory / "package.json").is_file() else {}
        repository = manifest.get("repository")
        repository = repository.get("url") if isinstance(repository, dict) else repository

        def read(files=files, name=name, version=version) -> list[Text]:
            if files:
                return [Text(path.name, decode(path.read_bytes())) for path in files]
            return clarified_texts(clarified[name], version) if name in clarified else []
        found.append(Component(
            "npm", f"{name.replace('/', '__')}-{version}", name, version,
            f"npm: {name} {version}" + (f", {repository}" if repository else ""),
            "the builder's interface", read, bool(files), entry.get("license") or "",
            clarified.get(name, {}).get("note", "")))
    return found


# Fonts ---------------------------------------------------------------------

def font_names(path: Path) -> dict[int, str]:
    """The name table of a TrueType or OpenType font: its family (1, or 16
    where a typographic family is given) and version (5)."""
    data = path.read_bytes()
    tables = int.from_bytes(data[4:6], "big")
    for index in range(tables):
        record = data[12 + 16 * index: 28 + 16 * index]
        if record[:4] == b"name":
            offset = int.from_bytes(record[8:12], "big")
            break
    else:
        raise FetchError(f"{path} has no name table")
    count = int.from_bytes(data[offset + 2:offset + 4], "big")
    strings = offset + int.from_bytes(data[offset + 4:offset + 6], "big")
    names: dict[int, str] = {}
    for index in range(count):
        record = data[offset + 6 + 12 * index: offset + 18 + 12 * index]
        platform, name = int.from_bytes(record[0:2], "big"), int.from_bytes(record[6:8], "big")
        length, start = int.from_bytes(record[8:10], "big"), int.from_bytes(record[10:12], "big")
        raw = data[strings + start: strings + start + length]
        text = raw.decode("utf-16-be") if platform in (0, 3) else raw.decode("latin-1")
        names.setdefault(name, text)
    return names


def font_user(path: str) -> str:
    parts = PurePosixPath(path).parts
    if parts[:2] == ("integrations", "designs"):
        return f"the {parts[2]} menu design, in every game exported with it"
    if parts[:2] == ("desktop", "src"):
        return "the builder's interface"
    return str(PurePosixPath(path).parent)


def fonts() -> list[Component]:
    """Every font in the repository, with the licence file beside it:
    `<Family>-OFL.txt`, `OFL.txt` or a LICENSE, named after the font's file
    up to its first hyphen or not at all."""
    tracked = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "--", *(f"*{s}" for s in FONT_SUFFIXES)],
                             capture_output=True, text=True, encoding="utf-8", check=True).stdout.split("\0")
    families: dict[str, list[str]] = {}
    for relative in (path for path in tracked if path and not path.startswith(("vendor/", "work/"))):
        names = font_names(ROOT / relative)
        families.setdefault(names.get(16) or names[1], []).append(relative)
    found = []
    for family, paths in sorted(families.items()):
        licences: list[Path] = []
        for relative in paths:
            font = ROOT / relative
            stem = font.stem.split("-")[0].lower()
            licences += [candidate for candidate in sorted(font.parent.iterdir())
                         if candidate.is_file() and candidate != font
                         and re.match(rf"^(?:{re.escape(stem)}[-_])?(ofl|licen[cs]e|copying)", candidate.name.lower())]
        versions = sorted({font_names(ROOT / relative).get(5, "unknown") for relative in paths})

        def read(licences=licences) -> list[Text]:
            texts: dict[str, list[str]] = {}
            for path in licences:
                texts.setdefault(decode(path.read_bytes()), []).append(path.relative_to(ROOT).as_posix())
            return [Text(", ".join(origins), body) for body, origins in texts.items()]
        found.append(Component(
            "fonts", re.sub(r"[^a-z0-9]+", "-", family.lower()).strip("-"), family, "; ".join(versions),
            ", ".join(paths), "; ".join(sorted({font_user(path) for path in paths})), read, True,
            "OFL-1.1" if licences and "OPEN FONT LICENSE Version 1.1" in licences[0].read_text(errors="replace")
            else ""))
    return found


def discover() -> list[Component]:
    """Every component, in the order the entries are written."""
    listed = declared()
    return [*declared_components("native", listed["native"]), *cores(), *crates(), *rust_std(), *npm(),
            *fonts(), *declared_components("data", listed["data"])]
