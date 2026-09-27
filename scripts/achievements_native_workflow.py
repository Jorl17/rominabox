"""Exercise managed achievements through the exported native player.

Requires a RetroArch build compiled with RIB_ACHIEVEMENTS_TEST in both C and
C++ sources, with only ROMINABOX_RA_TEST_HOST=http://127.0.0.1:PORT allowed.
We use the quiet, self-terminating native launch of menu_shots, which opens a
transparent window briefly. Run only when native launches have been approved.
"""

from __future__ import annotations

import argparse
import base64
from contextlib import contextmanager
from enum import IntEnum
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import threading
import time
import zlib
from urllib.parse import parse_qs

import menu_shots as shots
from achievements_native_rom import make_achievement_rom


ROOT = Path(__file__).resolve().parent.parent
CHECKPOINT = re.compile(r"\[RIB\] checkpoint (\S+) (\{.*\})")
TEST_MARKER = b"ROMINABOX_RA_TEST_HOST"

# Whether a game in its sandbox can reach the fake service of this test on
# 127.0.0.1, on each platform. In the macOS sandbox, a game with network
# access can reach loopback. In a Windows sandbox (AppContainer) it never can,
# because no capability exists for it. Only a loopback exemption helps, which
# requires an administrator (NetworkIsolationSetAppContainerConfig and
# CheckNetIsolation from Microsoft) and applies to the whole machine. So on
# Windows we run the game of this test outside the sandbox. We test the
# achievements client (hash, session, save and restore, unlock, OFF,
# exclusion), which works the same either way. In the isolation tests we
# check the effect of the sandbox on signing in (the accounts folder, and
# nothing beside it). A shipped game reaches the actual service over the
# internet, and internet access works in the sandbox.
SANDBOX_REACHES_LOOPBACK = {"macos": True, "windows": False}
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9h+AAAAABJRU5ErkJggg=="
)

class Status(IntEnum):
    EXCLUDED = 0
    OFF = 2
    ACTIVE = 5


class RowState(IntEnum):
    LOCKED = 0
    UNLOCKED = 1


class FixtureService(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self) -> None:
        super().__init__(("127.0.0.1", 0), FixtureHandler)
        self.requests: list[str] = []
        self.lock = threading.Lock()
        self.login_delay = 0.0
        self.hash: str | None = None
        self.earned = False

    def record(self, route: str) -> None:
        with self.lock:
            self.requests.append(route)

    def seen(self) -> list[str]:
        with self.lock:
            return self.requests.copy()

    def clear(self) -> None:
        with self.lock:
            self.requests.clear()


class FixtureHandler(BaseHTTPRequestHandler):
    server: FixtureService

    def log_message(self, format: str, *args: object) -> None:
        pass  # Never log the synthetic token or request body.

    def answer(self, body: bytes, content_type: str) -> None:
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        if not self.path.startswith(("/Badge/", "/Images/")):
            self.send_error(404)
            return
        self.server.record("badge")
        self.answer(PNG, "image/png")

    def do_POST(self) -> None:
        length = int(self.headers.get("Content-Length", "0"))
        if length > 8192:
            self.send_error(413)
            return
        fields = parse_qs(self.rfile.read(length).decode("ascii"))
        route = fields.get("r", [""])[0]
        self.server.record(route)
        if route == "login2":
            if fields.get("u") != ["Fixture"] or fields.get("t") != ["fixture-token"]:
                self.send_error(403)
                return
            time.sleep(self.server.login_delay)
            answer: dict = {"Success": True, "User": "Fixture", "Token": "fixture-token"}
        elif route == "gameid":
            digest = fields.get("m", [""])[0]
            if not re.fullmatch(r"[0-9a-f]{32}", digest):
                self.send_error(400)
                return
            if self.server.hash and digest != self.server.hash:
                self.send_error(400)
                return
            self.server.hash = digest
            answer = {"Success": True, "GameID": 1}
        elif route == "achievementsets":
            answer = {
                "Success": True, "GameId": 1, "Title": "Native fixture",
                "ConsoleId": 1, "ImageIconUrl": "/Images/1.png",
                "RichPresencePatch": "", "Sets": [{
                    "AchievementSetId": 1, "GameId": 1, "Title": "Native fixture",
                    "Type": "core", "ImageIconUrl": "/Images/1.png",
                    "Achievements": [{
                        "ID": 123, "Title": "Real memory", "Description": "Reach work RAM one",
                        # In Genesis Plus GX, work RAM is in word-swapped bytes.
                        "Flags": 3, "Points": 5, "MemAddr": "0xH0001=1.300.",
                        "Author": "Fixture", "BadgeName": "123", "Created": 1,
                        "Modified": 1,
                    }], "Leaderboards": [],
                }],
            }
        elif route == "startsession":
            answer = {"Success": True, "Unlocks": [{"ID": 123, "When": 1}] if self.server.earned else [], "HardcoreUnlocks": []}
        elif route == "awardachievement":
            if fields.get("a") != ["123"] or fields.get("h") != ["0"]:
                self.send_error(400)
                return
            self.server.earned = True
            answer = {"Success": True, "AchievementID": 123, "Score": 0,
                      "SoftcoreScore": 5, "AchievementsRemaining": 0}
        elif route == "ping":
            answer = {"Success": True}
        else:
            self.send_error(404)
            return
        self.answer(json.dumps(answer).encode(), "application/json")


@contextmanager
def fixture_service():
    service = FixtureService()
    worker = threading.Thread(target=service.serve_forever, daemon=True)
    worker.start()
    try:
        yield service
    finally:
        service.shutdown()
        service.server_close()
        worker.join(timeout=5)


def reaching_the_service(app: Path) -> None:
    """The game, able to reach the fake service. Where the sandbox blocks it
    (SANDBOX_REACHES_LOOPBACK), we run it outside the sandbox, with a launch
    plan without the line that requests one."""
    if shots.PLATFORM not in SANDBOX_REACHES_LOOPBACK:
        raise SystemExit(f"whether a sandboxed game reaches loopback is not declared for {shots.PLATFORM}")
    if SANDBOX_REACHES_LOOPBACK[shots.PLATFORM]:
        return
    plan = shots.resources_of(app) / "launch.plan"
    lines = plan.read_text(encoding="utf-8").splitlines(keepends=True)
    plan.write_text("".join(line for line in lines if not line.startswith("sandbox\t")),
                    encoding="utf-8", newline="")
    if shots.sandboxed(app):
        raise AssertionError(f"{app} still asks for its sandbox")


def owned_storage(app: Path, rom: bytes) -> Path:
    data = shots.data_dir_of(app)
    home = shots.storage_home(app)
    if not data or home is None:
        raise AssertionError("native fixture requires contained per-game storage")
    home = home.resolve()
    for current in (data, *data.parents):
        if shots.redirected(current):
            raise AssertionError(f"link in fixture storage path: {current}")
    if not data.resolve().is_relative_to(home):
        raise AssertionError(f"fixture storage escapes its games' folder: {data}")
    marker = data / "achievements-native-owner"
    owner = f"{ROOT}\n{hashlib.sha256(rom).hexdigest()}\n"
    if data.exists() and any(data.iterdir()):
        if shots.redirected(marker) or not marker.is_file() or marker.read_text() != owner:
            raise AssertionError(f"unowned fixture storage: {data}")
        states = data / "states"
        if shots.redirected(states):
            raise AssertionError(f"link in fixture storage: {states}")
        if states.exists():
            for state in states.iterdir():
                if shots.redirected(state) or not state.is_file() or \
                        not state.name.startswith("achievement-native.state"):
                    raise AssertionError(f"unexpected fixture state: {state}")
                state.unlink()
    # We decided whose storage it is from what was there. We prepare it in the
    # game before we write the marker and the session, because registering the
    # sandbox at the first launch of a Windows game empties its storage.
    shots.prepare_storage(app)
    data.mkdir(parents=True, exist_ok=True)
    marker.write_text(owner)
    return data


def session(data: Path, enabled: bool) -> None:
    path = data / "achievements.session"
    if shots.redirected(path):
        raise AssertionError(f"link in fixture storage: {path}")
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    # Only this user may read it: through its mode on macOS and Linux, and on
    # Windows through the access list of the per-user folder, without a mode.
    if hasattr(os, "fchmod"):
        os.fchmod(descriptor, 0o600)
    # LF on every platform, as in the player.
    with os.fdopen(descriptor, "w", encoding="ascii", newline="\n") as file:
        file.write(f"Fixture\nfixture-token\n{int(enabled)}\n")


def run_case(app: Path, output: Path, name: str, script: list[str]) -> dict[str, dict]:
    output.mkdir(parents=True, exist_ok=True)
    problem = shots.take(app, name, script, output, reset_settings=False)
    if problem:
        raise AssertionError(f"{name}: {problem}")
    reports = {label: json.loads(value) for label, value in
               CHECKPOINT.findall((output / f"{name}.log").read_text())}
    if "Test build refused a non-loopback" in (output / f"{name}.log").read_text():
        raise AssertionError(f"{name}: native HTTP boundary rejected a non-loopback URL")
    wanted = [step[7:] for step in script if step.startswith("report:")]
    if list(reports) != wanted or any("achievements" not in row for row in reports.values()):
        raise AssertionError(f"{name}: missing native achievement checkpoints {wanted}: {reports}")
    if "playing" in reports:
        assert not reports["playing"]["menuOpen"], "Continue reopened the startup menu"
    return {label: value["achievements"] for label, value in reports.items()}


def state_blocks(path: Path) -> dict[bytes, bytes]:
    """Read blocks from our generated RetroArch state, including RZIP compression."""
    data = path.read_bytes()
    if data.startswith(b"#RZIPv\x01#"):
        size = int.from_bytes(data[12:20], "little")
        offset, chunks = 20, []
        while offset < len(data):
            length = int.from_bytes(data[offset:offset + 4], "little")
            offset += 4
            chunks.append(zlib.decompress(data[offset:offset + length]))
            offset += length
        data = b"".join(chunks)
        assert len(data) == size, "incomplete compressed state"
    assert data[:8] == b"RASTATE\x01", "unexpected native save format"
    blocks, offset = {}, 8
    while offset < len(data):
        name = data[offset:offset + 4]
        length = int.from_bytes(data[offset + 4:offset + 8], "little")
        blocks[name] = data[offset + 8:offset + 8 + length]
        offset += 8 + ((length + 7) & ~7)
    return blocks


def one_row(report: dict, state: RowState) -> None:
    if report["status"] != Status.ACTIVE or len(report["rows"]) != 1 or \
            report["rows"][0]["id"] != 123 or report["rows"][0]["state"] != state:
        raise AssertionError(f"unexpected achievement row: {report}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path,
                        default=ROOT / "work/test-output/achievements-native")
    args = parser.parse_args()
    if not os.environ.get("ROMINABOX_GAME_BUNDLE_PREFIX"):
        parser.error("run eval \"$(python3 scripts/worktree.py env)\" first")
    selected = os.environ.get("ROMINABOX_TEST_BUILD")
    if not selected:
        parser.error("ROMINABOX_TEST_BUILD must name the exact test build")
    build_info = json.loads((Path(selected) / "build-info.json").read_text())
    if build_info.get("testOnly") is not True:
        parser.error("the selected build is not marked testOnly")
    player = shots.built_player()
    if TEST_MARKER not in player.read_bytes():
        parser.error("ROMINABOX_TEST_BUILD must name the exact test-guarded RetroArch build")

    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    rom_bytes = make_achievement_rom()
    rom = output / "achievement-native.md"
    rom.write_bytes(rom_bytes)
    # Separate this deterministic fixture from the ordinary screenshot identity.
    os.environ["ROMINABOX_GAME_BUNDLE_PREFIX"] += ".achievements-native"

    with fixture_service() as service:
        os.environ["ROMINABOX_RA_TEST_HOST"] = f"http://127.0.0.1:{service.server_port}"
        with shots.build_a_game(rom, output, settings={
            "includeAchievements": True, "autosaveOnQuit": True,
            "keepPlayingInBackground": True,
        }) as app:
            reaching_the_service(app)
            data = owned_storage(app, rom_bytes)
            session(data, True)
            first = run_case(app, output, "partial", [
                "report:initial", "wait-ms:250", "resume", "report:playing", "wait:150", "report:partial",
            ])
            one_row(first["partial"], RowState.LOCKED)
            if "awardachievement" in service.seen():
                raise AssertionError("achievement awarded before saved hit count completed")
            states = list((data / "states").glob("*.state.auto"))
            if len(states) != 1 or not state_blocks(states[0]).get(b"ACHV"):
                raise AssertionError("quit autosave did not persist native achievement progress")

            service.clear()
            service.login_delay = 1.2
            restored = run_case(app, output, "restore", [
                "report:connecting", "wait-ms:1800", "report:restored",
                "wait-ms:250", "resume", "report:playing", "wait:100", "report:before-award", "wait:180", "report:after-award",
            ])
            if not restored["connecting"]["startupWaiting"]:
                raise AssertionError("autoload did not wait for the account session")
            if restored["restored"]["startupWaiting"]:
                raise AssertionError("startup gate did not finish after the save task")
            one_row(restored["before-award"], RowState.LOCKED)
            one_row(restored["after-award"], RowState.UNLOCKED)
            if service.seen().count("awardachievement") != 1:
                raise AssertionError(f"expected one native award after restore: {service.seen()}")

            service.login_delay = 0
            active = run_case(app, output, "active", [
                "wait-ms:500", "options", "achievements", "report:active",
            ])
            one_row(active["active"], RowState.UNLOCKED)
            if service.seen().count("awardachievement") != 1:
                raise AssertionError("reopening replayed an already earned award")

            service.clear()
            session(data, False)
            off = run_case(app, output, "off", ["report:off", "wait-ms:250", "resume", "report:playing", "wait:400", "report:after"])
            if off["after"]["status"] != Status.OFF or service.seen():
                raise AssertionError(f"OFF evaluated or contacted the service: {off}, {service.seen()}")
            run_case(app, output, "off-screen", ["options", "achievements", "report:off"])
            if service.seen():
                raise AssertionError("opening the OFF screen contacted the service")

        service.clear()
        with shots.build_a_game(rom, output, settings={"includeAchievements": False}) as excluded_app:
            excluded = run_case(excluded_app, output, "excluded", [
                "report:excluded", "resume", "wait:400", "report:after",
            ])
            if excluded["after"]["status"] != Status.EXCLUDED or service.seen():
                raise AssertionError(f"excluded build contacted the service: {excluded}, {service.seen()}")
    print("native achievements: hash, session, save/restore, unlock, OFF and excluded passed")


if __name__ == "__main__":
    main()
