"""Check the image count of a multi-disc game in the core, and that choosing
the second image changes the disc index.

We show a disc entry in the menu only when get_num_images of the core returns
more than one, and choosing a row there calls CMD_EVENT_DISK_INDEX. In this
harness, outside RetroArch, we cannot call that command, so we use the same
disk callbacks in the same order: eject, then set the index. We read the
index before and after from the core, and never make up a number here.

A cartridge has no disc tray, and the tray of a single generated cue has one
image, so neither is a multi-disc game. The playlist is one, and after
someone chooses disc 2 (the second image), the index is 1.

In PCSX ReARMed, an m3u counts only after the first image passes CheckCdrom
and LoadCdrom. We read what these checks require in the core's code and then
tried it: a MODE1/2048 cue whose file is a multiple of 2048 bytes and does
not start with the raw sync word, an ISO9660 primary descriptor at sector 16,
a SYSTEM.CNF that lists PSX.EXE;1, and a PS-X EXE. The HLE BIOS is enough.
We run no frames in the harness, but the exported game runs, so the
executable is a program with one jump to itself. With no program at all,
the core ran empty memory, and its dynarec crashed on Windows. Once the first
image is loaded, get_num_images is the number of lines in the playlist.

We then export the same playlist with the menu-shot exporter in every design
with a disc list. We open disc 2 with a menu script, and the player's log
must then contain core image 1 of 2. In a cartridge export we must not show
the Disc entry, and the DISC button must open the circle.

We tell whether an entry is visible in Options by where the keyboard focus
goes. Pressing Down in Options reaches every entry that we show and none
that we hide, and we turn a paged list to the page of the focused entry, so
a hidden entry cannot pass for one shown on another page. With
`--without-player` we run only the cores in the frame harness.

    uv run python scripts/test_discs.py
    uv run python scripts/test_discs.py --without-player
"""

from __future__ import annotations

import functools
import io
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

sys.path.insert(0, str(Path(__file__).resolve().parent))
import frame_harness  # noqa: E402
import free_space  # noqa: E402
import exported_game  # noqa: E402
import menu_shots  # noqa: E402
import player_support  # noqa: E402
import prepare_runtime  # noqa: E402
import scratch  # noqa: E402
import toolchain  # noqa: E402
from core_source import core as local_core, host_target  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
CARTRIDGE = ROOT / "scripts/fixtures/test-game.gbc"
# The label from the core is this file name without the extension. We put
# the disc number at the end because we shorten long names in the middle in
# the menu to keep the end of a name visible. A name that ended before the
# number would not test that.
STEM = "ROM in a Box (Europe) (En,Fr,De,Es,It,Nl,Pt,Sv,No,Da,Fi,Pl)"
FIXTURE = ROOT / "work/i5-discs"
LIBRETRO = ROOT / "vendor/retroarch/libretro-common/include"
HARNESS = toolchain.executable(FIXTURE / "frame_harness")
SECTOR = 2048


def both_endian(value: int, width: int) -> bytes:
    return value.to_bytes(width, "little") + value.to_bytes(width, "big")


def directory_record(name: bytes, extent: int, size: int, flags: int) -> bytes:
    record = bytearray(33 + len(name))
    record[2:10] = both_endian(extent, 4)
    record[10:18] = both_endian(size, 4)
    record[25] = flags
    record[28:32] = both_endian(1, 2)
    record[32] = len(name)
    record[33:] = name
    if len(record) % 2:
        record.append(0)
    record[0] = len(record)
    return bytes(record)


def disc_image() -> bytes:
    """Return one data track for the core, with a program that loops forever.

    Sector 16 is the volume descriptor. The root directory record in it
    points to sector 18. Next come SYSTEM.CNF and the executable, which is a
    header and then one sector of program. Its first instruction is a jump to
    itself, with an empty delay slot.
    """
    configuration = (
        b"BOOT = cdrom:\\PSX.EXE;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFFF0\r\n"
    )
    start = 0x80010000
    executable = bytearray(2 * SECTOR)
    executable[0:8] = b"PS-X EXE"
    executable[0x10:0x14] = start.to_bytes(4, "little")
    executable[0x18:0x1C] = start.to_bytes(4, "little")
    executable[0x1C:0x20] = SECTOR.to_bytes(4, "little")
    executable[0x30:0x34] = (0x801FFF00).to_bytes(4, "little")
    # MIPS j: opcode 2 and the word address of the target within its 256 MB.
    executable[SECTOR : SECTOR + 4] = (0x08000000 | ((start & 0x0FFFFFFF) >> 2)).to_bytes(4, "little")

    root = b"".join(
        (
            directory_record(b"\x00", 18, SECTOR, 2),
            directory_record(b"\x01", 18, SECTOR, 2),
            directory_record(b"SYSTEM.CNF;1", 19, len(configuration), 0),
            directory_record(b"PSX.EXE;1", 20, len(executable), 0),
        )
    )
    descriptor = bytearray(SECTOR)
    descriptor[0] = 1
    descriptor[1:6] = b"CD001"
    descriptor[6] = 1
    descriptor[40:72] = b"ROMINABOX".ljust(32)
    sectors = 20 + len(executable) // SECTOR
    descriptor[80:88] = both_endian(sectors, 4)
    descriptor[120:124] = both_endian(1, 2)
    descriptor[124:128] = both_endian(1, 2)
    descriptor[128:132] = both_endian(SECTOR, 2)
    root_record = directory_record(b"\x00", 18, SECTOR, 2)
    descriptor[156 : 156 + len(root_record)] = root_record
    terminator = bytearray(SECTOR)
    terminator[0] = 255
    terminator[1:6] = b"CD001"
    terminator[6] = 1

    image = bytearray(sectors * SECTOR)
    image[16 * SECTOR : 17 * SECTOR] = descriptor
    image[17 * SECTOR : 18 * SECTOR] = terminator
    image[18 * SECTOR : 18 * SECTOR + len(root)] = root
    image[19 * SECTOR : 19 * SECTOR + len(configuration)] = configuration
    image[20 * SECTOR : sectors * SECTOR] = executable
    return bytes(image)


def write_disc(number: int, image: bytes) -> None:
    stem = f"{STEM} (Disc {number})"
    (FIXTURE / f"{stem}.bin").write_bytes(image)
    (FIXTURE / f"{stem}.cue").write_text(
        f'FILE "{stem}.bin" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\n',
        encoding="utf-8", newline="\n",
    )


def write_playlist(name: str, count: int) -> Path:
    path = FIXTURE / name
    lines = [f"{STEM} (Disc {number}).cue" for number in range(1, count + 1)]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    return path


def compile_harness() -> None:
    FIXTURE.mkdir(parents=True, exist_ok=True)
    frame_harness.compile_to(FIXTURE / "frame_harness")


def core_for(content: Path) -> Path:
    # Without its boot ROM, Gambatte does not load this cartridge. In mGBA it
    # loads with no disc tray, which is what we check for a cartridge.
    return core_of("mgba" if content.suffix == ".gbc" else "pcsx_rearmed")


@functools.cache
def core_of(component: str) -> Path:
    """Return the core of the component for this machine, by its name in the catalog."""
    return local_core(prepare_runtime.catalog_components()[component]["artifacts"][host_target()])


def phases_of(text: str) -> dict[str, dict]:
    found: dict[str, dict] = {}
    current = None
    for line in text.splitlines():
        if line.startswith("disc-phase "):
            current = line.split(" ", 1)[1]
            found[current] = {"images": None, "index": None, "labels": []}
            continue
        if current is None:
            continue
        if line.startswith("disc-images "):
            found[current]["images"] = int(line.split()[1])
        elif line.startswith("disc-index "):
            found[current]["index"] = int(line.split()[1])
        elif line.startswith("disc-label "):
            parts = line.split(" ", 2)
            found[current]["labels"].append(parts[2] if len(parts) > 2 else "")
    return found


def run(content: Path, disc: int | None = None) -> dict[str, dict]:
    command = [
        str(HARNESS),
        "--core",
        str(core_for(content)),
        "--content",
        str(content),
        "--frames",
        "0",
        "--system-dir",
        str(FIXTURE / "system"),
        "--save-dir",
        str(FIXTURE / "save"),
    ]
    if disc is not None:
        command += ["--disc", str(disc)]
    (FIXTURE / "system").mkdir(parents=True, exist_ok=True)
    (FIXTURE / "save").mkdir(parents=True, exist_ok=True)
    completed = subprocess.run(command, capture_output=True, text=True, timeout=180)
    if completed.returncode != 0:
        sys.stderr.write(completed.stderr)
        raise SystemExit(
            f"the core refused {content.name} (exit {completed.returncode})"
        )
    phases = phases_of(completed.stdout)
    if "before" not in phases or "after" not in phases:
        sys.stderr.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        raise SystemExit(f"{content.name} produced no disc report")
    return phases


# We keep the disc entry disabled until we have the image count from the
# core, and we ignore a click before then. The log then contains image 0, as
# for a row whose action never ran. We take the options picture after the
# same wait, and the entry must show in it, so a shorter wait fails there.
LOAD_FRAMES = 240
PLAYER = ROOT / "work" / "discs-player"
SHOTS = PLAYER / "shots"
# The line we write in the fork (runloop.c) after the tray has closed on the
# chosen disc, when the second image is in and not only selected.
SWAPPED = "tray closed, core image 1 of 2"
# The tray closes a fixed number of frames after a swap in RetroArch. We
# wait this long after the click before we end the run, and we print SWAPPED
# in the player when the tray has closed in that time. So a longer delay
# fails as a missing line, never as a wrong index.
AFTER_SWAP_FRAMES = 240


def require_disk() -> None:
    """The space we need for the exports in the temporary directory and for the shots."""
    free_space.require(22, Path(tempfile.gettempdir()), PLAYER)


@functools.cache
def design_screens(design: str) -> list[dict]:
    """Return the screens of a design as we resolve them in the menu, which are
    the Native screens merged by id with those of the design. A design that
    only restyles Native declares none and still has all of them."""
    from built import cli

    request = json.dumps({"design": str(ROOT / "integrations/designs" / design)})
    ran = subprocess.run([str(cli()), "design-screens"], input=request, capture_output=True,
                         text=True, check=True)
    return json.loads(ran.stdout)["result"]["screens"]


def disc_list_id() -> str:
    """Return the screen id of the disc list. In Native that screen has the discs
    role, and a design that restyles it declares the same id."""
    return next(screen["id"] for screen in design_screens("native") if screen.get("role") == "discs")


def disc_designs() -> list[str]:
    """Return the designs with a disc list, in their order in designs.json."""
    declared = json.loads((ROOT / "desktop/designs.json").read_text(encoding="utf-8"))["designs"]
    found = []
    for entry in declared:
        screens = design_screens(entry["id"])
        if any(screen.get("id") == disc_list_id() for screen in screens):
            found.append(entry["id"])
    if not found:
        raise SystemExit("no design declares a disc list")
    return found


def listing_of(screens: list[dict]) -> dict:
    return next(screen for screen in screens if screen.get("id") == disc_list_id())


def redirect_of(screens: list[dict], listing_id: str) -> dict | None:
    return next(
        (screen for screen in screens if screen.get("images") == listing_id),
        None,
    )


def swap_steps(design: str) -> list[str]:
    """Go on from the open Pause screen, through Options in Native or directly with a column button."""
    screens = design_screens(design)
    listing = listing_of(screens)
    row = f"{listing['id']}-1"
    redirect = redirect_of(screens, listing["id"])
    if redirect is not None:
        return [redirect["button"], row]
    options = next(screen for screen in screens if screen.get("place") == "options")
    return [options["button"], listing["button"], row]


def one_disc_steps(design: str) -> list[str]:
    screens = design_screens(design)
    listing = listing_of(screens)
    redirect = redirect_of(screens, listing["id"])
    if redirect is not None:
        return [redirect["button"]]
    options = next(screen for screen in screens if screen.get("place") == "options")
    return [options["button"]]


def index_extension() -> str:
    """Return the file extension of a game's tray record in RetroArch.

    We declare it in the fork in file_path_special.h, which is compiled into
    RetroArch. We compile the same header here and print the value, so we use
    the name from the player and not a copy of it.
    """
    probe_source = (
        '#include "file_path_special.h"\n#include <stdio.h>\n'
        "int main(void) { fputs(FILE_PATH_DISK_CONTROL_INDEX_EXTENSION, stdout); return 0; }\n"
    )
    with scratch.scratch("rominabox-discs-index-") as made:
        probe = toolchain.executable(Path(made) / "index-extension")
        built = subprocess.run(
            [toolchain.describe()["cc"], "-x", "c", "-", "-o", str(probe),
             "-I", str(ROOT / "vendor/retroarch"), "-I", str(LIBRETRO)],
            input=probe_source, capture_output=True, text=True, timeout=60,
        )
        if built.returncode != 0:
            raise SystemExit(f"could not compile file_path_special.h:\n{built.stderr[-600:]}")
        extension = subprocess.run([str(probe)], capture_output=True, text=True,
                                   check=True, timeout=15).stdout
    if not extension.startswith("."):
        raise SystemExit(f"the disc index record has no extension: {extension!r}")
    return extension


def script_of(steps: list[str], after: int | None = None) -> str:
    parts = [f"wait:{LOAD_FRAMES}", *steps]
    if after:
        parts.append(f"wait:{after}")
    return ",".join(parts)


def frame_limit(script: str) -> str:
    waits = [
        int(part.split(":", 1)[1])
        for part in script.split(",")
        if part.startswith("wait:")
    ]
    # The frames before the menu script starts also count, so with a limit
    # equal to the waits the player exits before we write the done line.
    return str(sum(waits) + 800)


def system_for(content: Path) -> str:
    """Return the console of this file.

    Most extensions belong to one console. For a shared playlist extension we
    take the console whose shipped core is the dylib loaded in the harness.
    """
    extension = content.suffix.lstrip(".")
    systems = json.loads((ROOT / "desktop/systems.json").read_text(encoding="utf-8"))["systems"]
    claimed = [system for system in systems if extension in system.get("extensions", [])]
    if len(claimed) == 1:
        return claimed[0]["id"]
    core_name = core_for(content).name
    matches = []
    for system in claimed:
        for core in system.get("cores", []):
            if core_name in core.get("artifacts", {}).values():
                matches.append(system["id"])
                break
    if len(matches) != 1:
        raise SystemExit(f"{content.name} matches {matches or 'no system'}, not one")
    return matches[0]


def palette_id() -> str:
    names = menu_shots.declared_palettes()
    return names[0]


def palette_color(name: str) -> tuple[int, int, int]:
    declared = json.loads((ROOT / "desktop/designs.json").read_text(encoding="utf-8"))["palettes"]
    entry = next(item for item in declared if item["id"] == palette_id())
    text = entry[name].removeprefix("#")
    return tuple(int(text[index : index + 2], 16) for index in (0, 2, 4))


def fresh_player_dir() -> None:
    if PLAYER.is_symlink() or (PLAYER.exists() and not PLAYER.is_dir()):
        raise SystemExit(f"{PLAYER} is not the discs output directory")
    SHOTS.mkdir(parents=True, exist_ok=True)


def forget_tray_record(app: Path) -> None:
    """With a saved index we restore the last disc, so choosing disc 2 changes nothing.

    This is a problem here, because the native launch has already written the
    record for this content path.
    """
    data = exported_game.data_dir_of(app)
    if data is None:
        return
    saves = data / "saves"
    if not saves.is_dir():
        return
    extension = index_extension()
    for record in saves.glob(f"*{extension}"):
        if record.is_file() and not record.is_symlink():
            record.unlink()


def leftover_problem(app: Path) -> str | None:
    left = exported_game.running_from(app)
    return f"a player is still running:\n{left}" if left else None


def launch(app: Path, script: str, shot: Path | None, output_too: bool = False) -> str:
    """Drive the menu and let the frame limit end the process.

    We report a timeout or a leftover process with its PID, and leave the
    process running so that someone can inspect it.
    """
    if exported_game.storage_home(app) is None:
        raise SystemExit(f"{app.name} keeps its storage uncontained; refusing to launch")
    log = exported_game.log_of(app)
    if log and log.exists():
        log.unlink()
    forget_tray_record(app)
    exported_game.prepared_storage(app)
    inside = shot
    if shot is not None:
        shot.parent.mkdir(parents=True, exist_ok=True)
        shot.unlink(missing_ok=True)
        inside = exported_game.shot_inside(app, shot)
    env = {
        **os.environ,
        "ROMINABOX_MAX_FRAMES": frame_limit(script),
        exported_game.SCRIPT_ENV: script,
        exported_game.quiet_env(): "1",
    }
    if inside is not None:
        env["ROMINABOX_MENU_SHOT"] = str(inside.resolve())
    print(f"  {script}", flush=True)
    with tempfile.TemporaryFile(mode="w+t") as capture:
        process = subprocess.Popen(
            [str(exported_game.launcher_of(app))],
            stdout=capture,
            stderr=capture,
            text=True,
            env=env,
        )
        try:
            process.wait(timeout=90)
        except subprocess.TimeoutExpired:
            capture.seek(0, os.SEEK_END)
            capture.seek(max(capture.tell() - 800, 0))
            partial = capture.read()
            left = leftover_problem(app)
            raise menu_shots.PlayerTimeout(
                f"the player did not exit after the frame limit (pid {process.pid}); "
                "left running for inspection\n"
                f"{left or ''}\n{partial}",
                app,
            ) from None
        capture.seek(0)
        output = capture.read()
    written = log.read_text(errors="replace") if log and log.exists() else ""
    if inside is not None and shot is not None:
        exported_game.carry_shot(inside, shot)
    left = leftover_problem(app)
    if left:
        raise menu_shots.PlayerTimeout(left, app)
    if shot is not None and not shot.exists():
        tail = "\n".join(written.strip().splitlines()[-8:])
        launch_output = "\n".join((output or "").strip().splitlines()[-12:])
        raise SystemExit(
            f"no screenshot was written (exit {process.returncode})\n{tail}\n{launch_output}"
        )
    if shot is None and "core image" not in written and "names no element" not in written:
        launch_output = "\n".join((output or "").strip().splitlines()[-20:])
        raise SystemExit(
            f"the player did not report a core image (exit {process.returncode})\n"
            f"{written[-800:]}\n{launch_output}"
        )
    # We print menu script checkpoints to the player output, not to its log.
    return written + "\n" + (output or "") if output_too else written


def image_lines(written: str) -> list[str]:
    return [line for line in written.splitlines() if "core image" in line]


def require_swap(design: str, written: str) -> str | None:
    lines = image_lines(written)
    print(f"{design}:")
    for line in lines:
        print(line)
    if any("names no element" in line for line in written.splitlines()):
        missed = next(line for line in written.splitlines() if "names no element" in line)
        return missed.split("[RIB]")[-1].strip()
    if any(SWAPPED in line for line in lines):
        return None
    shown = "\n".join(lines) if lines else "(no core image line)"
    return f"{design}: the log does not say {SWAPPED}:\n{shown}"


@contextmanager
def export_game(content: Path, design: str) -> Iterator[Path]:
    """Return a game of `content` in `design`, stored apart from the game of the
    same content in any other design, so that we can run them all at once."""
    require_disk()
    print(f"export {design} {content.name}", flush=True)
    with menu_shots.build_a_game(
        content,
        PLAYER,
        system=system_for(content),
        design=design,
        palette=palette_id(),
        namespace=f".{design}",
    ) as app:
        yield app


def assets_of(app: Path) -> Path:
    """Return the composed menu that we put in an export."""
    return exported_game.resources_of(app) / "menu-assets"


def menu_asset(app: Path, name: str) -> str:
    path = assets_of(app) / name
    if not path.is_file():
        raise SystemExit(f"the export has no {name}")
    return path.read_text(encoding="utf-8")


def rule_body(css: str, selector: str) -> str:
    # Otherwise we would read a comment as part of the selector name, because
    # the block above #options-entries has no braces.
    css = re.sub(r"/\*.*?\*/", "", css, flags=re.DOTALL)
    for match in re.finditer(r"([^{}]+)\{([^{}]*)\}", css):
        selectors = [part.strip() for part in match.group(1).split(",")]
        if selector in selectors:
            return match.group(2)
    raise SystemExit(f"the stylesheet has no {selector}")


def css_prop(body: str, name: str) -> str | None:
    found = re.search(rf"(?:^|;)\s*{re.escape(name)}\s*:\s*([^;]+)", body)
    return found.group(1).strip() if found else None


def dp(value: str) -> float:
    match = re.search(r"-?\d+(?:\.\d+)?", value)
    if not match:
        raise SystemExit(f"not a length: {value}")
    return float(match.group(0))


def border_dp(body: str) -> float:
    declared = css_prop(body, "border-width") or css_prop(body, "border")
    return dp(declared) if declared else 0.0


def length_px(value: str, scale: float, extent: int) -> float:
    if value.endswith("%"):
        return extent * float(value[:-1]) / 100.0
    return dp(value) * scale


def screen_origin(css: str, scale: float, width: int, height: int) -> tuple[float, float]:
    body = rule_body(css, "#screen")
    left = length_px(css_prop(body, "left") or "0", scale, width)
    top = length_px(css_prop(body, "top") or "0", scale, height)
    margin_left = css_prop(body, "margin-left")
    margin_top = css_prop(body, "margin-top")
    if margin_left:
        left += dp(margin_left) * scale
    if margin_top:
        top += dp(margin_top) * scale
    border = border_dp(body) * scale
    return left + border, top + border


def near(pixel: tuple[int, ...], colour: tuple[int, int, int]) -> bool:
    return all(abs(channel - wanted) <= 28 for channel, wanted in zip(pixel[:3], colour))


def window_dp(app: Path) -> tuple[int, int]:
    text = exported_game.plan_text(app)
    width = re.search(r'video_windowed_position_width = "(\d+)"', text)
    height = re.search(r'video_windowed_position_height = "(\d+)"', text)
    if not width or not height:
        raise SystemExit("the export does not say its window size")
    return int(width.group(1)), int(height.group(1))


def circle_geometry(css: str) -> tuple[float, float, float, float, float]:
    """Return the largest rounded box in the stylesheet, which is the disc face and not the hole."""
    best = None
    for match in re.finditer(r"([^{}]+)\{([^{}]*)\}", css):
        body = match.group(2)
        if "border-radius" not in body or css_prop(body, "width") is None:
            continue
        width = dp(css_prop(body, "width") or "0")
        if best is None or width > best[0]:
            best = (
                width,
                dp(css_prop(body, "left") or "0"),
                dp(css_prop(body, "top") or "0"),
                dp(css_prop(body, "height") or "0"),
                border_dp(body),
            )
    if best is None:
        raise SystemExit("the stylesheet has no circle")
    width, left, top, height, border = best
    return left, top, width, height, border


def circle_open(image, css: str, app: Path) -> bool:
    window_width, _height = window_dp(app)
    scale = image.size[0] / window_width
    left_dp, top_dp, width_dp, height_dp, border = circle_geometry(css)
    origin_x, origin_y = screen_origin(css, scale, image.size[0], image.size[1])
    left = int(origin_x + left_dp * scale)
    top = int(origin_y + top_dp * scale)
    right = int(left + width_dp * scale)
    bottom = int(top + height_dp * scale)
    highlight = palette_color("highlight")
    pixels = image.load()
    hit = 0
    for y in range(max(top, 0), min(bottom, image.size[1])):
        for x in range(max(left, 0), min(right, image.size[0])):
            if near(pixels[x, y], highlight):
                hit += 1
    outer = width_dp * height_dp * scale * scale
    inner_w = max(width_dp - 2 * border, 0) * scale
    inner_h = max(height_dp - 2 * border, 0) * scale
    ring = outer - inner_w * inner_h
    print(f"  circle highlight pixels {hit}, ring about {ring:.0f}")
    return hit > ring * 0.4


def open_image(path: Path):
    from PIL import Image

    return Image.open(path).convert("RGB")


def option_entries(app: Path) -> list[str]:
    """Return the Options entries in the game's menu, in their order."""
    return re.findall(
        r'<button\b(?=[^>]*\bclass="[^"]*\boption-entry\b)[^>]*\bid="([^"]+)"',
        menu_asset(app, "menu.rml"),
    )


def focus_walk(app: Path, screens: list[dict], shot: Path) -> list[str]:
    """Return the Options entries that the keyboard focus reaches, in order,
    when we open Options and press Down once per entry in the menu, plus once.

    The focus goes to every entry that we show and never to a hidden one, and
    we turn a paged list to the page of the focused entry, so we read what we
    show whatever page an entry is on. We keep a picture of the last page."""
    options = next(screen["button"] for screen in screens if screen.get("place") == "options")
    entries = option_entries(app)
    steps = [options, "report:open"]
    for index in range(len(entries) + 1):
        steps += ["key:down", f"report:down{index}"]
    output = launch(app, script_of(steps), shot, output_too=True)
    reached: list[str] = []
    walked: list[str] = []
    for line in output.splitlines():
        found = re.search(r"\[RIB\] checkpoint \S+ (\{.*\})\s*$", line)
        if not found:
            continue
        focused = json.loads(found.group(1)).get("focused", [])
        walked.append("/".join(focused) or "-")
        for element in focused:
            if element in entries and element not in reached:
                reached.append(element)
    print(f"  focus went {' > '.join(walked)}")
    return reached


class JobOutput(io.TextIOBase):
    """Standard output kept apart for each job that runs beside others, so that
    we print the lines of a job together when it ends."""

    def __init__(self, real) -> None:
        self.real = real
        self.local = threading.local()

    def write(self, text: str) -> int:
        buffer = getattr(self.local, "buffer", None)
        return (buffer if buffer is not None else self.real).write(text)

    def flush(self) -> None:
        if getattr(self.local, "buffer", None) is None:
            self.real.flush()


def two_disc_game(playlist: Path, design: str, apps: list[Path]) -> list[str]:
    """Check that choosing disc 2 swaps the image in the core, and that Options lists Disc."""
    with export_game(playlist, design) as app:
        apps.append(app)
        written = launch(
            app,
            script_of(swap_steps(design), AFTER_SWAP_FRAMES),
            SHOTS / f"{design}-two-list.png",
        )
        problem = require_swap(design, written)
        if problem:
            return [problem]
        screens = design_screens(design)
        listing = listing_of(screens)
        if listing.get("option"):
            reached = focus_walk(app, screens, SHOTS / f"{design}-two-options.png")
            if listing["button"] not in reached:
                return [f"{design}: a two-disc game does not show the Disc entry"]
    return []


def one_disc_game(design: str, apps: list[Path]) -> list[str]:
    """Check that we hide the Disc entry for a cartridge, and that pressing DISC opens the circle."""
    with export_game(CARTRIDGE, design) as app:
        apps.append(app)
        screens = design_screens(design)
        listing = listing_of(screens)
        redirect = redirect_of(screens, listing["id"])
        shot = SHOTS / f"{design}-one.png"
        launch(app, script_of(one_disc_steps(design)), shot)
        if listing.get("option"):
            reached = focus_walk(app, screens, SHOTS / f"{design}-one-options.png")
            if listing["button"] in reached:
                return ["a one-disc game must not show the entry"]
            # The focus went past the place of the entry, so it is hidden
            # and not on a page that we did not turn to. When an export has
            # no such entry at all, there is nothing to hide.
            entries = option_entries(app)
            after = (entries[entries.index(listing["button"]) + 1:]
                     if listing["button"] in entries else reached)
            if not any(entry in reached for entry in after):
                return ["focus never passed the Disc entry's place in Options, so the walk proves nothing"]
        if redirect is not None:
            if not circle_open(open_image(shot), menu_asset(app, "menu.rcss"), app):
                return ["the DISC button did not open the circle"]
    return []


def exported_player(playlist: Path) -> list[str]:
    """Export and launch the two-disc and one-disc game of every design at once.
    We store each game apart (export_game), and print the lines of each game
    together, in this order, when all have ended."""
    require_disk()
    fresh_player_dir()
    before = player_support.snapshot()
    found: list[str] = []
    exported_apps: list[Path] = []
    games = [functools.partial(two_disc_game, playlist, design, exported_apps)
             for design in disc_designs()]
    games += [functools.partial(one_disc_game, design, exported_apps)
              for design in disc_designs()]
    output = JobOutput(sys.stdout)

    def run_alone(game) -> tuple[str, list[str], BaseException | None]:
        buffer = output.local.buffer = io.StringIO()
        try:
            problems = game()
            return buffer.getvalue(), problems, None
        except BaseException as error:  # reported after every game's lines
            return buffer.getvalue(), [], error
        finally:
            output.local.buffer = None

    failure: BaseException | None = None
    sys.stdout = output
    try:
        with ThreadPoolExecutor(max_workers=len(games)) as pool:
            ended = list(pool.map(run_alone, games))
    finally:
        sys.stdout = output.real
        for app in exported_apps:
            left = exported_game.running_from(app)
            if left:
                found.append(f"a player is still running:\n{left}")
        after = player_support.snapshot()
        leaked = player_support.additions(before, after) + player_support.modifications(
            before, after
        )
        if leaked:
            found.append(
                "an exported launch wrote the account ROM-in-a-Box directory: "
                + ", ".join(leaked[:8])
            )
    for text, problems, error in ended:
        print(text, end="", flush=True)
        found.extend(problems)
        failure = failure or error
    if failure is not None:
        raise failure
    return found


def main() -> None:
    without_player = "--without-player" in sys.argv[1:]
    if not CARTRIDGE.is_file():
        raise SystemExit(f"the cartridge is not at {CARTRIDGE}")
    compile_harness()
    image = disc_image()
    for number in range(1, 5):
        write_disc(number, image)
    single = FIXTURE / f"{STEM} (Disc 1).cue"
    two = write_playlist("two-discs.m3u", 2)
    four = write_playlist("four-discs.m3u", 4)

    cartridge = run(CARTRIDGE)
    images = cartridge["before"]["images"]
    if images is None or images > 1:
        raise SystemExit(
            f"the test cartridge reported {images} images; a cartridge is not a multi-disc game"
        )

    one = run(single)
    if one["before"]["images"] != 1:
        raise SystemExit(
            f"a single disc reported {one['before']['images']} images"
        )

    swapped = run(two, disc=1)
    before = swapped["before"]["index"]
    after = swapped["after"]["index"]
    print(f"two-disc before index {before}, after index {after}")
    if swapped["before"]["images"] != 2:
        raise SystemExit(
            f"a two-disc playlist reported {swapped['before']['images']} images"
        )
    index_problem = None
    if before != 0 or after != 1:
        index_problem = (
            f"after choosing disc 2 the core still reports image index {after}, not 1"
        )

    listed = run(four)
    labels = listed["before"]["labels"]
    print("four-disc labels:")
    for label in labels:
        print(f"  {label}")
    if listed["before"]["images"] != 4 or len(labels) != 4:
        raise SystemExit(
            f"a four-disc playlist reported {listed['before']['images']} images"
        )
    for number, label in enumerate(labels, start=1):
        if f"(Disc {number})" not in label:
            raise SystemExit(
                f"disc {number} label lost its number: {label}"
            )

    launched = [] if without_player else exported_player(two)
    problems = [item for item in (index_problem, *launched) if item]
    if problems:
        raise SystemExit("\n".join(problems))
    if without_player:
        print("the cores pass; the exported player was not launched")


if __name__ == "__main__":
    main()
