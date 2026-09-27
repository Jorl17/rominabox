"""Launch the exported player for the menu workflow cases marked `launched`.

We replay every case in scripts/fixtures/menu-workflows.json headlessly in
the `workflows` tests (desktop/src-tauri/tests/menu_workflows.rs). Here we run
the cases listed under `launched` in the table, one launch each, in the
exported player, and compare them with the same baselines: every checkpoint
from the script driver of the player, the files written by its menu, and
the picture. We leave out what the `headless` section of the table lists, so
the record is the same for both runners, and we compare the picture only here.

Use an explicit committed native build, with the worktree environment loaded:
    ROMINABOX_TEST_BUILD=/absolute/build python3 scripts/menu_workflows.py
    ROMINABOX_TEST_BUILD=/absolute/build python3 scripts/menu_workflows.py --record

We use menu_shots for export, launch, capture and lifetime management, and
keep the picture, script and checkpoint log of every case. With --record we
change the baselines, and only for the launched cases. Sound, physical
capture and native focus/fullscreen are manual.
"""

from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

from PIL import Image

import menu_shots as shots
from make_test_rom import make_megadrive_rom

ROOT = shots.ROOT
TABLE = ROOT / "scripts/fixtures/menu-workflows.json"
OUTPUT = ROOT / "work/test-output/menu-workflows"
CHECKPOINT = re.compile(r"\[RIB\] checkpoint (\S+) (\{.*\})")


def fixture(directory: Path) -> Path:
    """The original cartridge, without an account or a service fixture."""
    directory.mkdir(parents=True, exist_ok=True)
    rom = directory / "menu.md"
    rom.write_bytes(make_megadrive_rom())
    return rom


def reset_fixture(app: Path) -> None:
    """Reset only the menu files of the generated cartridge, never a supplied app.

    We pass in only exports built here from make_megadrive_rom(). Worktree
    isolation must be active. The files in the state directory of the fixture
    are diagnostic saves we can make again, not user content.
    """
    claim_fixture(app)
    data = shots.data_dir_of(app)
    if data is None or shots.storage_home(app) is None:
        raise SystemExit("the generated fixture must have contained game storage")
    session = data / "achievements.session"
    if session.exists() or session.is_symlink():
        raise SystemExit(f"fixture account storage is not signed out; no reset performed: {session}")
    for name in ("volume.cfg", "background-play.cfg", "shader-choice"):
        path = data / name
        if path.is_symlink():
            raise SystemExit(f"refusing a symlink in fixture storage: {path}")
        path.unlink(missing_ok=True)
    states = data / "states"
    if states.is_symlink():
        raise SystemExit(f"refusing a symlink in fixture storage: {states}")
    if states.is_dir():
        for path in states.iterdir():
            if path.is_file() and not path.is_symlink():
                path.unlink()


def claim_fixture(app: Path) -> None:
    """Claim empty fixture storage once, and refuse unknown menu data already there."""
    data = shots.data_dir_of(app)
    # The folder that contains a game's storage on each platform: a sandboxed
    # macOS game's container, or the games folder in Windows' per-user data.
    sandbox = shots.storage_home(app)
    if data is None or data.is_symlink() or sandbox is None:
        raise SystemExit("the fixture requires its own contained game storage")
    if not data.is_relative_to(sandbox):
        raise SystemExit(f"fixture storage is outside its sandbox: {data}")
    for current in (data, *data.parents):
        if current.is_symlink():
            raise SystemExit(f"refusing a symlink in fixture storage: {current}")
    if not data.resolve().is_relative_to(sandbox.resolve()):
        raise SystemExit(f"fixture storage resolves outside its sandbox: {data}")
    marker = data / "menu-workflow-owner"
    owner = f"{ROOT}\n{hashlib.sha256(make_megadrive_rom()).hexdigest()}\n"
    if marker.is_symlink():
        raise SystemExit(f"refusing a symlink ownership marker: {marker}")
    if marker.exists():
        if marker.read_text(encoding="utf-8") != owner:
            raise SystemExit(f"fixture storage has a different owner: {data}")
        return
    existing = [data / name for name in ("volume.cfg", "background-play.cfg", "controls.cfg",
                                         "shader-choice", "achievements.session")]
    for name in ("states", "remaps"):
        directory = data / name
        if directory.is_symlink():
            raise SystemExit(f"refusing a symlink in fixture storage: {directory}")
        if directory.is_dir():
            existing += [path for path in directory.rglob("*") if not path.is_dir()]
    unknown = [path for path in existing if path.exists() or path.is_symlink()]
    if unknown:
        raise SystemExit("pre-existing unowned fixture files; no reset performed:\n"
                         + "\n".join(str(path) for path in unknown))
    data.mkdir(parents=True, exist_ok=True)
    marker.write_text(owner, encoding="utf-8", newline="\n")


def persisted(app: Path, table: dict) -> dict[str, str]:
    """The files listed in the table, from the game's data, as written by the player."""
    data = shots.data_dir_of(app)
    assert data is not None
    # Each export has a separate temporary directory. Canonicalize only this
    # exact app prefix, and keep all filenames, relative paths and other bytes
    # as they are. A path into any other app will still differ.
    app_prefix = str(app.resolve()) + "/"
    paths = sorted({path for pattern in table["persisted"] for path in data.glob(pattern)})
    # Written with `/` on every platform, as in the rules of the table.
    return {path.relative_to(data).as_posix(): path.read_text(encoding="utf-8").replace(app_prefix, "$APP/")
            for path in paths if path.is_file()}


def declared(table: dict, design: str, palette: str, name: str) -> dict:
    """One case of the table for one design and palette, in the headless runner's form."""
    shots_declared = shots.declared_shots()
    for group in table["groups"]:
        for entry in group["cases"]:
            if entry["name"] != name:
                continue
            shot = shots_declared[entry["shot"]] if "shot" in entry else {}
            script = entry.get("script", shot.get("script"))
            if isinstance(script, dict):
                script = script[design]
            steps = []
            for step in script:
                steps += table["sequences"][step[1:]][design] if step.startswith("@") else [step]
            export = {**table["export"], **table["designs"][design].get("export", {}),
                      **group.get("export", {}),
                      **{key: value for key, value in shot.items() if key not in ("script", "config", "inMotion")},
                      **entry.get("export", {}), "theme": design, "palette": palette}
            if "continues" in entry:
                raise SystemExit(f"{name} continues another case; launch that case first")
            return {"key": f"{design}/{palette}/{name}", "baseline": group["baseline"],
                    "script": steps, "export": export, "inMotion": shot.get("inMotion", False)}
    raise SystemExit(f"the table declares no case {name}")


def reduced(record: dict, table: dict) -> dict:
    """A record as we compare it in both runners. For each value that the table's
    `headless` section lists as launched-only, we keep only whether it is there."""
    reductions = table["headless"]["reports"]
    reports = {}
    for label, report in record["reports"].items():
        report = json.loads(json.dumps(report))
        for path in reductions:
            parent, _, field = path.rpartition(".")
            holder = report[parent] if parent else report
            if isinstance(holder, dict) and field in holder:
                value = holder[field]
                holder[field] = bool(value) if isinstance(value, (list, str, bool)) else value is not None
        reports[label] = report
    files = {}
    for name, text in record["files"].items():
        rule = next((what for prefix, what in table["headless"]["files"].items()
                     if prefix == name or (prefix.endswith("/") and name.startswith(prefix))), None)
        if isinstance(rule, str):
            continue
        if rule:
            text = "".join(line + "\n" for line in text.splitlines() if not line.startswith(rule["lines"]))
        files[name] = text
    return {"script": record["script"], "reports": reports, "files": files}


def checkpoints(log: Path) -> dict[str, dict]:
    """The reports from the player in its log, by checkpoint label."""
    return {label: json.loads(value) for label, value in CHECKPOINT.findall(log.read_text(encoding="utf-8", errors="replace"))}


def capture(app: Path, destination: Path, case: dict, table: dict) -> dict:
    name = case["key"].rsplit("/", 1)[1]
    reset_fixture(app)
    script = case["script"] + table["end"]
    failure = shots.take(app, name, script, destination, reset_settings=True)
    if failure:
        raise SystemExit(f"{case['key']}: {failure}")
    reports = checkpoints(destination / f"{name}.log")
    wanted = [step[7:] for step in script if step.startswith("report:")]
    if list(reports) != wanted:
        raise SystemExit(f"{case['key']}: expected checkpoints {wanted}, got {list(reports)}")
    picture = destination / f"{name}.png"
    record = reduced({"script": case["script"], "reports": reports, "files": persisted(app, table)}, table)
    if not case["inMotion"]:
        record["picture"] = hashlib.sha256(picture.read_bytes()).hexdigest()
        record["size"] = list(Image.open(picture).size)
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record", action="store_true", help="replace the launched cases' baseline entries")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    arguments = parser.parse_args()
    if not os.environ.get("ROMINABOX_TEST_BUILD"):
        raise SystemExit("select the exact committed player with ROMINABOX_TEST_BUILD")
    if not os.environ.get("ROMINABOX_GAME_BUNDLE_PREFIX", "").startswith("app.rominabox.game.wt-"):
        raise SystemExit("load scripts/worktree.py env before running the generated fixture")
    player = shots.built_player()  # validates build-info against this fork's HEAD
    dirty = subprocess.check_output(["git", "-C", str(ROOT / "vendor/retroarch"),
                                     "status", "--porcelain", "--untracked-files=no"], text=True)
    if dirty:
        raise SystemExit("commit the fork and build it before recording/comparing workflows")
    table = json.loads(TABLE.read_text(encoding="utf-8"))
    cases = [declared(table, item["design"], item["palette"], item["case"]) for item in table["launched"]]
    if not cases:
        raise SystemExit("the table launches no case")
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    rom = fixture(output / "input")
    provenance = json.loads((player.parents[1] / "build-info.json").read_text(encoding="utf-8"))
    provenance["binarySha256"] = hashlib.sha256(player.read_bytes()).hexdigest()
    provenance["cliSha256"] = hashlib.sha256(shots.command().read_bytes()).hexdigest()
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8", newline="\n")

    results: dict[str, dict] = {}
    with ExitStack() as exports:
        games: dict[str, Path] = {}
        for case in cases:
            key = json.dumps(case["export"], sort_keys=True)
            if key not in games:
                games[key] = exports.enter_context(shots.build_a_game(rom, output, settings=case["export"]))
            destination = output / case["key"].rsplit("/", 1)[0]
            destination.mkdir(parents=True, exist_ok=True)
            results[case["key"]] = capture(games[key], destination, case, table)
            print(f"captured {case['key']}", flush=True)
    (output / "results.json").write_text(json.dumps(results, indent=2, sort_keys=True) + "\n",
                                         encoding="utf-8", newline="\n")

    changed = []
    baselines: dict[str, dict] = {}
    for case in cases:
        baseline = baselines.setdefault(case["baseline"], json.loads((ROOT / case["baseline"]).read_text(encoding="utf-8")))
        before = baseline.get(case["key"])
        after = results[case["key"]]
        if arguments.record:
            baseline[case["key"]] = after
            continue
        if before is None:
            changed.append(case["key"])
            print(f"CHANGED {case['key']}: nothing recorded; inspect {output} and --record")
            continue
        compared = reduced(before, table)
        compared.update({field: before[field] for field in ("picture", "size") if field in before})
        fields = [field for field in sorted(compared.keys() | after.keys()) if compared.get(field) != after.get(field)]
        if fields:
            changed.append(case["key"])
            print(f"CHANGED {case['key']}: {', '.join(fields)}; inspect its log and picture in {output}")
    if arguments.record:
        for file, baseline in baselines.items():
            # We write these files in the headless runner too, in this form.
            (ROOT / file).write_text(json.dumps(baseline, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
                                     encoding="utf-8", newline="\n")
        print(f"recorded {len(results)} launched cases; inspect the pictures before accepting them")
        return 0
    if changed:
        return 1
    print(f"{len(results)} launched menu cases match their baselines")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
