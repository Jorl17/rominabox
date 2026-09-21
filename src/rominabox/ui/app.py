from __future__ import annotations

import sys
import tempfile
from pathlib import Path

from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import QApplication, QMessageBox, QWidget

from ..domain import Project
from ..paths import resources, runtime_kit
from .player import Player


def run(smoke_output: Path | None = None, *, player_only: bool = False) -> int:
    """Open a generated game's player or the authoring app based on bundled resources."""
    app = QApplication(sys.argv[:1])
    app.setApplicationName("ROM-in-a-Box")
    app.setOrganizationName("ROM-in-a-Box")
    # We hide the player's only window on purpose while RetroArch is running.
    app.setQuitOnLastWindowClosed(False)
    storage = tempfile.TemporaryDirectory(prefix="rominabox-smoke-") if smoke_output else None
    try:
        manifest = resources() / "player.json"
        if player_only or manifest.exists():
            project = Project.read(manifest)
            if smoke_output:
                project.show_menu, project.fullscreen = True, False
            window: QWidget = Player(project, runtime_kit(), Path(storage.name) if storage else None)
        else:
            from .builder import Builder

            window = Builder()
        window.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        window.destroyed.connect(app.quit)
        window.show()
        if smoke_output:

            def capture() -> None:
                window.grab().save(str(smoke_output))
                window.close()

            QTimer.singleShot(250, capture)
        return app.exec()
    except Exception as exc:
        if smoke_output:
            print(str(exc), file=sys.stderr)
        else:
            QMessageBox.critical(None, "ROM-in-a-Box", str(exc))
        return 1
    finally:
        if storage:
            storage.cleanup()
