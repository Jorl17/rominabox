"""Failure-path checks for native harnesses, with no player process launched."""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import patch

import menu_shots
import menu_workflows
import test_discs


class FakePlayer:
    pid = 4242

    def __init__(self) -> None:
        self.signals: list[str] = []

    def wait(self, timeout: int) -> int:
        raise subprocess.TimeoutExpired("fake-player", timeout)

    def kill(self) -> None:
        self.signals.append("kill")

    def terminate(self) -> None:
        self.signals.append("terminate")

    def send_signal(self, _signal: int) -> None:
        self.signals.append("signal")


class FakeCompletedPlayer:
    pid = 4243
    returncode = 0

    def wait(self, timeout: int) -> int:
        return self.returncode


class NativeHarnessTimeoutTest(unittest.TestCase):
    def test_owned_export_is_removed_after_normal_use(self) -> None:
        real_mkdtemp = tempfile.mkdtemp
        with tempfile.TemporaryDirectory() as parent:
            def make_dir(*, prefix: str) -> str:
                return real_mkdtemp(prefix=prefix, dir=parent)

            def fake_export(_rom: Path, _workspace: Path, run_dir: Path, *_args: object) -> Path:
                app = run_dir / "Game.app"
                app.mkdir()
                return app

            with (
                patch.object(menu_shots.tempfile, "mkdtemp", side_effect=make_dir),
                patch.object(menu_shots, "_build_a_game", side_effect=fake_export),
            ):
                with menu_shots.build_a_game(Path("/fake/game"), Path("/fake/workspace")) as app:
                    self.assertTrue(app.is_dir())
                self.assertFalse(app.exists())

    def test_owned_export_stays_when_its_player_times_out(self) -> None:
        real_mkdtemp = tempfile.mkdtemp
        with tempfile.TemporaryDirectory() as parent:
            def make_dir(*, prefix: str) -> str:
                return real_mkdtemp(prefix=prefix, dir=parent)

            def fake_export(_rom: Path, _workspace: Path, run_dir: Path, *_args: object) -> Path:
                app = run_dir / "Game.app"
                app.mkdir()
                return app

            with (
                patch.object(menu_shots.tempfile, "mkdtemp", side_effect=make_dir),
                patch.object(menu_shots, "_build_a_game", side_effect=fake_export),
            ):
                with self.assertRaises(menu_shots.PlayerTimeout):
                    with menu_shots.build_a_game(Path("/fake/game"), Path("/fake/workspace")) as app:
                        raise menu_shots.PlayerTimeout("fake timeout", app)
                self.assertTrue(app.is_dir())

    def test_timeout_retains_only_the_matching_export(self) -> None:
        real_mkdtemp = tempfile.mkdtemp
        with tempfile.TemporaryDirectory() as parent:
            def make_dir(*, prefix: str) -> str:
                return real_mkdtemp(prefix=prefix, dir=parent)

            def fake_export(_rom: Path, _workspace: Path, run_dir: Path, *_args: object) -> Path:
                app = run_dir / "Game.app"
                app.mkdir()
                return app

            with (
                patch.object(menu_shots.tempfile, "mkdtemp", side_effect=make_dir),
                patch.object(menu_shots, "_build_a_game", side_effect=fake_export),
            ):
                with self.assertRaises(menu_shots.PlayerTimeout):
                    with ExitStack() as exports:
                        running = exports.enter_context(
                            menu_shots.build_a_game(Path("/fake/one"), Path("/fake/workspace"))
                        )
                        idle = exports.enter_context(
                            menu_shots.build_a_game(Path("/fake/two"), Path("/fake/workspace"))
                        )
                        raise menu_shots.PlayerTimeout("fake timeout", running)
                self.assertTrue(running.is_dir())
                self.assertFalse(idle.exists())

    def test_menu_shot_success_still_returns_no_problem(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            def launch(_command: list[str], **options: object) -> FakeCompletedPlayer:
                Path(options["env"]["ROMINABOX_MENU_SHOT"]).write_bytes(b"screenshot")
                return FakeCompletedPlayer()

            with (
                patch.object(menu_shots, "launcher_of", return_value=Path("/fake/launcher")),
                patch.object(menu_shots, "log_of", return_value=None),
                patch.object(menu_shots, "data_dir_of", return_value=None),
                patch.object(menu_shots, "quiet_env", return_value="ROMINABOX_QUIET"),
                patch.object(menu_shots.subprocess, "Popen", side_effect=launch),
            ):
                problem = menu_shots.take(
                    Path("/fake/Game.app"), "success", [], Path(directory)
                )

        self.assertEqual(problem, "")

    def test_menu_shot_timeout_reports_pid_without_signaling(self) -> None:
        player = FakePlayer()
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(menu_shots, "launcher_of", return_value=Path("/fake/launcher")),
                patch.object(menu_shots, "log_of", return_value=None),
                patch.object(menu_shots, "data_dir_of", return_value=None),
                patch.object(menu_shots, "quiet_env", return_value="ROMINABOX_QUIET"),
                patch.object(menu_shots.subprocess, "Popen", return_value=player) as popen,
                patch.object(
                    menu_shots.subprocess,
                    "run",
                    side_effect=subprocess.TimeoutExpired("fake-player", 120),
                ),
            ):
                with self.assertRaises(SystemExit) as raised:
                    menu_shots.take(
                        Path("/fake/Game.app"), "timeout", [], Path(directory)
                    )

        self.assertIn("timed out", str(raised.exception))
        self.assertIn("4242", str(raised.exception))
        self.assertEqual(player.signals, [])
        self.assertNotEqual(popen.call_args.kwargs["stdout"], subprocess.PIPE)
        self.assertNotEqual(popen.call_args.kwargs["stderr"], subprocess.PIPE)

    def test_disc_leftover_reports_process_without_quitting(self) -> None:
        with (
            patch.object(menu_shots, "running_from", return_value="4242 /fake/launcher"),
            patch.object(test_discs.subprocess, "run") as run,
        ):
            problem = test_discs.leftover_problem(Path("/fake/Game.app"))

        self.assertIn("4242 /fake/launcher", problem)
        run.assert_not_called()

    def test_disc_timeout_reports_pid_without_signaling(self) -> None:
        player = FakePlayer()
        with (
            patch.object(menu_shots, "storage_home", return_value=Path("/fake")),
            patch.object(menu_shots, "log_of", return_value=None),
            patch.object(menu_shots, "launcher_of", return_value=Path("/fake/launcher")),
            patch.object(menu_shots, "quiet_env", return_value="ROMINABOX_QUIET"),
            patch.object(test_discs, "forget_tray_record"),
            patch.object(test_discs, "leftover_problem", return_value="4242 /fake/launcher"),
            patch.object(test_discs.subprocess, "Popen", return_value=player) as popen,
        ):
            with self.assertRaises(SystemExit) as raised:
                test_discs.launch(Path("/fake/Game.app"), "", None)

        self.assertIn("4242", str(raised.exception))
        self.assertIn("left running for inspection", str(raised.exception))
        self.assertEqual(popen.call_args.args[0], [str(Path("/fake/launcher"))])
        self.assertEqual(player.signals, [])
        self.assertNotEqual(popen.call_args.kwargs["stdout"], subprocess.PIPE)
        self.assertIs(popen.call_args.kwargs["stderr"], popen.call_args.kwargs["stdout"])


class WorkflowReportTest(unittest.TestCase):
    def test_a_report_reads_as_the_utf8_the_player_writes(self) -> None:
        # The name of the Mega Drive pad contains "·", and "Á" is a byte (0x81)
        # that we cannot decode at all in the Windows code page.
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "case.log"
            log.write_bytes('[RIB] checkpoint final {"text": "Mega Drive · 3 buttons, Á"}\n'.encode("utf-8"))
            reports = menu_workflows.checkpoints(log)
        self.assertEqual(reports, {"final": {"text": "Mega Drive · 3 buttons, Á"}})


def link_directory(link: Path, target: Path) -> None:
    """Make a folder that leads somewhere else, in the way an ordinary user can
    on this system: a symbolic link on macOS and Linux, and a junction on
    Windows, where a symbolic link requires a privilege."""
    if os.name == "posix":
        link.symlink_to(target, target_is_directory=True)
    elif os.name == "nt":
        import _winapi

        _winapi.CreateJunction(str(target), str(link))
    else:
        raise NotImplementedError(f"no way to link a folder on os.name {os.name!r}")


class GameStarted(Exception):
    pass


class HarnessLinkTest(unittest.TestCase):
    def test_a_shot_refuses_game_storage_that_leads_elsewhere(self) -> None:
        # For a shot we delete the game's log and remaps before we start the
        # game. Through a link, those would be the files of someone else.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            data = root / "data"
            data.mkdir()
            elsewhere = root / "elsewhere"
            elsewhere.mkdir()
            kept = elsewhere / "launch.log"
            kept.write_text("somebody else's log")
            link_directory(data / "logs", elsewhere)
            problem = None
            with (
                patch.object(menu_shots, "data_dir_of", return_value=data),
                patch.object(menu_shots, "log_of", return_value=data / "logs" / "launch.log"),
                patch.object(menu_shots, "launcher_of", return_value=Path("/fake/launcher")),
                patch.object(menu_shots.subprocess, "Popen", side_effect=GameStarted),
            ):
                try:
                    problem = menu_shots.take(Path("/fake/Game.app"), "shot", [], root / "out")
                except GameStarted:
                    pass
            self.assertTrue(kept.exists(), "a shot deleted a file through a link")
            self.assertIn("link", problem or "")


class WorkflowFixtureOwnershipTest(unittest.TestCase):
    def test_claim_refuses_redirected_storage_parent(self) -> None:
        for linked in ("Data", "Games"):
            with self.subTest(linked=linked), tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                sandbox = root / "Data"
                elsewhere = root / "elsewhere"
                elsewhere.mkdir()
                if linked == "Data":
                    link_directory(sandbox, elsewhere)
                else:
                    sandbox.mkdir()
                    link_directory(sandbox / "Games", elsewhere)
                with (
                    patch.object(menu_shots, "data_dir_of", return_value=sandbox / "Games" / "fixture"),
                    patch.object(menu_shots, "storage_home", return_value=sandbox),
                ):
                    with self.assertRaisesRegex(SystemExit, "link in fixture storage"):
                        menu_workflows.claim_fixture(Path("/fake/Fixture.app"))
                self.assertEqual(list(elsewhere.iterdir()), [])

    def test_reset_refuses_existing_unowned_saves(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory).resolve()
            (data / "states").mkdir()
            saved = data / "states" / "menu.state1"
            saved.write_bytes(b"unowned save")
            with (
                patch.object(menu_shots, "data_dir_of", return_value=data),
                patch.object(menu_shots, "storage_home", return_value=data),
            ):
                with self.assertRaisesRegex(SystemExit, "pre-existing unowned fixture files"):
                    menu_workflows.reset_fixture(Path("/fake/Fixture.app"))
            self.assertEqual(saved.read_bytes(), b"unowned save")

    def test_empty_claim_then_reset_only_fixture_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory).resolve()
            with (
                patch.object(menu_shots, "data_dir_of", return_value=data),
                patch.object(menu_shots, "storage_home", return_value=data),
            ):
                menu_workflows.claim_fixture(Path("/fake/Fixture.app"))
                (data / "states").mkdir()
                saved = data / "states" / "menu.state1"
                saved.write_bytes(b"generated fixture save")
                (data / "volume.cfg").write_text('audio_volume = "-80"\n')
                (data / "retroarch.cfg").write_text("launcher configuration\n")
                menu_workflows.reset_fixture(Path("/fake/Fixture.app"))
            self.assertFalse(saved.exists())
            self.assertFalse((data / "volume.cfg").exists())
            self.assertTrue((data / "menu-workflow-owner").is_file())
            self.assertEqual((data / "retroarch.cfg").read_text(), "launcher configuration\n")


if __name__ == "__main__":
    unittest.main()
