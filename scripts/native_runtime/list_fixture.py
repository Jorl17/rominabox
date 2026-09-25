"""Put a generated list screen into a composed menu.rml, for the bridge checks.

When we compose a menu, we make list screens only from actual data (shaders,
discs, achievements). For the bridge checks we need a list we control: rows
with and without a detail line, a one-line row, and optionally BACK. We put
it before the unlock row, where we put our own list screens. The bind list
is the one we compose, and we name two of its rows so that we can measure
the row edge.

    python3 scripts/native_runtime/list_fixture.py MENU_RML [--actions]
"""

from __future__ import annotations

import sys
from pathlib import Path

ANCHOR = '<div id="unlock-row"'


def row(name: str, title: str, detail: bool, line: bool) -> str:
    lines = " line" if line else " "
    parts = [f'<button id="fixture-{name}" class="list-row{lines}">',
             f'<div id="fixture-{name}-title" class="list-row-title">{title}</div>']
    if detail:
        parts.append(f'<div id="fixture-{name}-detail" class="list-row-detail">detail</div>')
    parts.append(f'<div id="fixture-{name}-state" class="list-row-state"></div></button>')
    return "".join(parts)


def panel(actions: bool) -> str:
    rows = (row("one", "ONE", False, True) + row("rest", "REST", False, True)
            + row("two", "TWO", True, False) + row("pic", "PIC", True, False))
    strip = ('<div class="list-actions">'
             '<button class="menu-action list-back" id="fixture-back">BACK</button></div>') if actions else ""
    return ('<div id="fixture-panel" class="screen-panel" style="display:none;">'
            f'<div class="list"><div id="fixture-page-1" class="list-page">{rows}</div></div>'
            f'{strip}</div>')


def main() -> int:
    path = Path(sys.argv[1])
    document = path.read_text()
    if ANCHOR not in document:
        raise SystemExit(f"{path} has no {ANCHOR}> to place the list fixture before")
    for index, title in ((1, "ONE"), (2, "REST")):
        empty = f'<div id="bind-{index}-title" class="list-row-title"></div>'
        if empty not in document:
            raise SystemExit(f"{path} has no empty bind-{index} row to name")
        document = document.replace(empty, empty.replace("></div>", f">{title}</div>"))
    path.write_text(document.replace(ANCHOR, panel("--actions" in sys.argv[2:]) + ANCHOR, 1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
