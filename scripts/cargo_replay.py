"""Run `cargo test` binaries again when their sources have not changed.

With a warm cache, `cargo test` of the engine crate takes about ten seconds
before any test runs, and the tests themselves take under a second. That
time goes into checking that the build is up to date. The binaries from the
last run are the same tests, so on a later run we execute them directly.

The stamp is a hash of the crate's rust sources, its manifests, the manifest
and lockfile of its workspace, and every file embedded in those sources with
include_str! or include_bytes!. We leave out a file that a test opens at
runtime, because we read its current copy in the test. With an embedded file left out, we would run a binary that
still has the previous copy, and the test would pass for the wrong reason.

We keep stamps in work/ and never commit them. We do not stamp a failed
cargo run, because the run ends at the first failing target and the list of
binaries would lack the targets after it.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
import threading
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
# The one pattern for include_str!/include_bytes!, which we use in both scripts
# that list the non-Rust files compiled into a binary, so their patterns
# cannot differ. It matches with or without a space after the `!`.
from built import compiled_in  # noqa: E402
import programs  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
STAMPS = ROOT / "work/cargo-stamps"
RUNNING = re.compile(r"^ *Running (.+?) \((.+)\)\s*$", re.M)
SKIP_DIRS = {"target", "resources", "node_modules", "gen"}
# We take the shared target lock only for an actual cargo run, not to run
# stamped binaries, so we can run those in several test scopes at once.
CARGO_LOCK = threading.Lock()
SOURCE_SUFFIXES = {".rs", ".toml", ".json", ".lock"}


class Plan:
    def __init__(self, manifest: Path, lib_only: bool, harness: list[str]):
        self.manifest = manifest
        self.lib_only = lib_only
        self.harness = harness


def parse(command: list[str]) -> Plan | None:
    """The cargo commands we use in this suite, or None to run cargo as given."""
    if len(command) < 2 or command[0] != "cargo" or command[1] != "test":
        return None
    tokens = command[2:]
    harness: list[str] = []
    if "--" in tokens:
        split = tokens.index("--")
        harness = tokens[split + 1 :]
        tokens = tokens[:split]
    manifest: Path | None = None
    lib_only = False
    positional: list[str] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if token in {"--quiet", "--offline"}:
            index += 1
            continue
        if token == "--lib":
            lib_only = True
            index += 1
            continue
        if token == "--manifest-path" and index + 1 < len(tokens):
            manifest = Path(tokens[index + 1])
            index += 2
            continue
        if token.startswith("-"):
            return None
        positional.append(token)
        index += 1
    if manifest is None:
        return None
    return Plan(manifest, lib_only, positional + harness)


def _rustc() -> str:
    asked = subprocess.run(["rustc", "--version"], capture_output=True, text=True, **programs.windowless())
    return asked.stdout.strip() if asked.returncode == 0 else "unknown"


def _package_files(package: Path) -> list[Path]:
    files: list[Path] = []
    if not package.is_dir():
        return files
    for directory, dirnames, filenames in os.walk(package):
        dirnames[:] = [name for name in dirnames if name not in SKIP_DIRS]
        for name in filenames:
            path = Path(directory) / name
            if path.suffix in SOURCE_SUFFIXES or path.name == "build.rs":
                files.append(path)
    return files


def _declares_workspace(manifest: Path) -> bool:
    return manifest.is_file() and "workspace" in tomllib.loads(manifest.read_text(encoding="utf-8"))


def workspace_files(package: Path) -> list[Path]:
    """The manifest and lockfile of the workspace that `package` belongs to,
    when that workspace's root is above it. They pin the package's
    dependencies and declare those it inherits. For a package that is its
    own workspace, both files are beside its sources."""
    if _declares_workspace(package / "Cargo.toml"):
        return []
    for folder in package.parents:
        if _declares_workspace(folder / "Cargo.toml"):
            return [folder / "Cargo.toml", folder / "Cargo.lock"]
        if folder == ROOT:
            break
    return []


def source_digest(manifest: Path) -> str:
    """Hash of everything that can change what the test binaries contain."""
    package = manifest.parent
    files = _package_files(package) + workspace_files(package)
    # A path dependency, such as rominabox-scratch, is outside the package. We
    # hash its sources as well, so after a change to it the stamp no longer
    # matches and we rebuild the binary.
    for match in re.finditer(r'path\s*=\s*"([^"]+)"', manifest.read_text()):
        dependency = (package / match.group(1)).resolve()
        files.extend(_package_files(dependency))
    embedded: list[Path] = []
    for path in files:
        if path.suffix == ".rs":
            embedded.extend(compiled_in(path))
    digest = hashlib.sha256()
    digest.update(_rustc().encode())
    for path in sorted(set(files + embedded), key=lambda item: str(item)):
        digest.update(str(path).encode())
        digest.update(path.read_bytes() if path.is_file() else b"<missing>")
    return digest.hexdigest()


def _stamp_path(plan: Plan) -> Path:
    key = hashlib.sha256(
        f"{plan.manifest.resolve()}|{plan.lib_only}".encode()
    ).hexdigest()[:16]
    return STAMPS / f"{key}.json"


def _load(plan: Plan, digest: str) -> list[dict] | None:
    path = _stamp_path(plan)
    if not path.is_file():
        return None
    try:
        saved = json.loads(path.read_text())
    except json.JSONDecodeError:
        return None
    if saved.get("source") != digest or saved.get("version") != 1:
        return None
    binaries = saved.get("binaries") or []
    for binary in binaries:
        file = Path(binary["path"])
        if not file.is_file():
            return None
        # The filename is cargo's metadata hash, which does not change with
        # the source, so we tie the stamp to a tree with the source digest
        # above. We take a file that is still here with the same size as the
        # binary we recorded in the stamp. We ignore mtime, because a later
        # `cargo test` of one target rewrites the file in place, and we would
        # discard the stamp on every run of the suite.
        if file.stat().st_size != binary["size"]:
            return None
    if plan.lib_only:
        binaries = [binary for binary in binaries if "src/lib.rs" in binary["label"]]
        if not binaries:
            return None
    return binaries


def _save(plan: Plan, digest: str, output: str) -> None:
    # `Doc-tests` is not a binary. A new doc-test changes a hashed source
    # file, so the stamp no longer matches and we run cargo.
    found = RUNNING.findall(output)
    if not found:
        return
    binaries = []
    for label, path in found:
        file = Path(path)
        if not file.is_file():
            return
        binaries.append(
            {
                "label": label,
                "path": str(file),
                "size": file.stat().st_size,
            }
        )
    STAMPS.mkdir(parents=True, exist_ok=True)
    _stamp_path(plan).write_text(
        json.dumps({"version": 1, "source": digest, "binaries": binaries}, indent=2) + "\n"
    )


def environment(env: dict[str, str] | None = None) -> dict:
    """This process's environment, with the variables we add for the scope."""
    return {**os.environ, **(env or {})}


def _replay(binaries: list[dict], harness: list[str], cwd: Path, env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    stdout: list[str] = ["replaying compiled tests; sources unchanged\n"]
    stderr: list[str] = []
    code = 0
    for binary in binaries:
        stdout.append(f"     Running {binary['label']} ({binary['path']})\n")
        ran = subprocess.run(
            [binary["path"], *harness],
            cwd=cwd,
            capture_output=True,
            text=True,
            errors="replace",
            env=environment(env),
            **programs.windowless(),
        )
        stdout.append(ran.stdout)
        stderr.append(ran.stderr)
        if ran.returncode != 0:
            code = ran.returncode
    return subprocess.CompletedProcess(["replay"], code, "".join(stdout), "".join(stderr))


def cargo_test(command: list[str], cwd: Path, env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    plan = parse(command)
    if plan is None or not plan.manifest.is_file():
        return subprocess.run(
            command, cwd=cwd, capture_output=True, text=True, errors="replace",
            env=environment(env), **programs.windowless(),
        )
    digest = source_digest(plan.manifest)
    saved = _load(plan, digest)
    if saved is not None:
        return _replay(saved, plan.harness, cwd, env)
    # We read the stamp from the "Running … (path)" lines, not printed with --quiet.
    visible = [token for token in command if token != "--quiet"]
    with CARGO_LOCK:
        ran = subprocess.run(
            visible, cwd=cwd, capture_output=True, text=True, errors="replace",
            env=environment(env), **programs.windowless(),
        )
    if ran.returncode == 0:
        _save(plan, digest, ran.stdout + ran.stderr)
    else:
        # After a failed run the stamped binary may be rebuilt, under the same
        # name, from different source. We do not stamp a failure, and the new
        # binary is often the same size, so once the source is back to what we
        # recorded (an edit undone, a branch switched back) the stamp would
        # match again and we would run the wrong binary. So we drop it.
        _stamp_path(plan).unlink(missing_ok=True)
    return ran
