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
a SYSTEM.CNF that lists PSX.EXE;1, and a PS-X EXE header with text size 0, so
that no program is copied. The HLE BIOS is enough. We run no frames in the
harness, so the stub never runs. Once the first image is loaded,
get_num_images is the number of lines in the playlist.

We then export the same playlist with the menu-shot exporter in every design
with a disc list. We open disc 2 with a menu script, and the player's log
must then contain core image 1 of 2. In a cartridge export we must not show
the Disc entry, and the DISC button must open the circle.

    python3 scripts/test_discs.py
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

sys.path.insert(0, str(Path(__file__).resolve().parent))
import menu_shots  # noqa: E402
import player_support  # noqa: E402
from core_source import core as local_core  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
CARTRIDGE = ROOT / "scripts/fixtures/test-game.gbc"
# The label from the core is this file name without the extension. We put
# the disc number at the end because we shorten long names in the middle in
# the menu to keep the end of a name visible. A name that ended before the
# number would not test that.
STEM = "ROM in a Box (Europe) (En,Fr,De,Es,It,Nl,Pt,Sv,No,Da,Fi,Pl)"
FIXTURE = ROOT / "work/i5-discs"
HARNESS_SOURCE = ROOT / "scripts/native_runtime/frame_harness.c"
LIBRETRO = ROOT / "vendor/retroarch/libretro-common/include"
HARNESS = FIXTURE / "frame_harness"
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
    """Return one data track with nothing to execute, for the core to load.

    Sector 16 is the volume descriptor. The root directory record in it
    points to sector 18. SYSTEM.CNF and the executable header follow. The
    text size of the executable is 0, so LoadCdrom returns after the header.
    """
    configuration = (
        b"BOOT = cdrom:\\PSX.EXE;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFFF0\r\n"
    )
    executable = bytearray(SECTOR)
    executable[0:8] = b"PS-X EXE"
    executable[0x10:0x14] = (0x80010000).to_bytes(4, "little")
    executable[0x18:0x1C] = (0x80010000).to_bytes(4, "little")
    executable[0x30:0x34] = (0x801FFF00).to_bytes(4, "little")

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
    descriptor[80:88] = both_endian(21, 4)
    descriptor[120:124] = both_endian(1, 2)
    descriptor[124:128] = both_endian(1, 2)
    descriptor[128:132] = both_endian(SECTOR, 2)
    root_record = directory_record(b"\x00", 18, SECTOR, 2)
    descriptor[156 : 156 + len(root_record)] = root_record
    terminator = bytearray(SECTOR)
    terminator[0] = 255
    terminator[1:6] = b"CD001"
    terminator[6] = 1

    image = bytearray(21 * SECTOR)
    image[16 * SECTOR : 17 * SECTOR] = descriptor
    image[17 * SECTOR : 18 * SECTOR] = terminator
    image[18 * SECTOR : 18 * SECTOR + len(root)] = root
    image[19 * SECTOR : 19 * SECTOR + len(configuration)] = configuration
    image[20 * SECTOR : 21 * SECTOR] = executable
    return bytes(image)


def write_disc(number: int, image: bytes) -> None:
    stem = f"{STEM} (Disc {number})"
    (FIXTURE / f"{stem}.bin").write_bytes(image)
    (FIXTURE / f"{stem}.cue").write_text(
        f'FILE "{stem}.bin" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\n'
    )


def write_playlist(name: str, count: int) -> Path:
    path = FIXTURE / name
    lines = [f"{STEM} (Disc {number}).cue" for number in range(1, count + 1)]
    path.write_text("\n".join(lines) + "\n")
    return path


def compile_harness() -> None:
    FIXTURE.mkdir(parents=True, exist_ok=True)
    compiled = subprocess.run(
        ["cc", "-O2", f"-I{LIBRETRO}", "-o", str(HARNESS), str(HARNESS_SOURCE)],
        capture_output=True,
        text=True,
    )
    if compiled.returncode != 0:
        sys.stderr.write(compiled.stderr)
        raise SystemExit("frame_harness did not compile")


def core_for(content: Path) -> Path:
    # Without its boot ROM, Gambatte does not load this cartridge. In mGBA it
    # loads with no disc tray, which is what we check for a cartridge.
    name = (
        "mgba_libretro.dylib"
        if content.suffix == ".gbc"
        else "pcsx_rearmed_libretro.dylib"
    )
    return local_core(name)


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
SWAPPED = "core image 1 of 2"


def free_gb() -> int:
    line = subprocess.run(
        ["df", "-g", "/Users/mariowilde"],
        capture_output=True,
        text=True,
        check=True,
        timeout=15,
    ).stdout.splitlines()[1]
    return int(line.split()[3])


def design_screens(design: str) -> list[dict]:
    path = ROOT / "integrations/designs" / design / "design.json"
    return json.loads(path.read_text())["screens"]


def disc_designs() -> list[str]:
    """Return the designs with a disc list, in their order in designs.json."""
    declared = json.loads((ROOT / "desktop/designs.json").read_text())["designs"]
    found = []
    for entry in declared:
        screens = design_screens(entry["id"])
        if any(screen.get("images") == "list" for screen in screens):
            found.append(entry["id"])
    if not found:
        raise SystemExit("no design declares a disc list")
    return found


def listing_of(screens: list[dict]) -> dict:
    return next(screen for screen in screens if screen.get("images") == "list")


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


def tray_countdown() -> int:
    """The number of frames between disk_control_set_index and the tray closing.

    The script must not end during that wait. Otherwise the done line contains
    the index while the tray is still open, and that is not the line we write
    in the fork once the disc is in.
    """
    text = (ROOT / "vendor/retroarch/disk_control_interface.c").read_text()
    found = re.search(r"pending_disk_control_insert\s*=\s*(\d+)", text)
    if not found:
        raise SystemExit("disk_control_set_index no longer waits before closing the tray")
    return int(found.group(1))


def index_extension() -> str:
    text = (ROOT / "vendor/retroarch/file_path_special.h").read_text()
    found = re.search(
        r'#define FILE_PATH_DISK_CONTROL_INDEX_EXTENSION "([^"]+)"',
        text,
    )
    if not found:
        raise SystemExit("the disc index record has no extension")
    return found.group(1)


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
    systems = json.loads((ROOT / "desktop/systems.json").read_text())["systems"]
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
    declared = json.loads((ROOT / "desktop/designs.json").read_text())["palettes"]
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
    data = menu_shots.data_dir_of(app)
    if data is None:
        return
    saves = data / "saves"
    if not saves.is_dir():
        return
    extension = index_extension()
    for record in saves.glob(f"*{extension}"):
        if record.is_file() and not record.is_symlink():
            record.unlink()


def still_running(app: Path) -> str:
    marker = str(app)
    found = subprocess.run(
        ["pgrep", "-fl", marker],
        capture_output=True,
        text=True,
        timeout=15,
    )
    return "\n".join(
        line for line in found.stdout.splitlines() if "pgrep" not in line
    )


def leftover_problem(app: Path) -> str | None:
    left = still_running(app)
    return f"a player is still running:\n{left}" if left else None


def launch(app: Path, script: str, shot: Path | None) -> str:
    """Drive the menu and let the frame limit end the process.

    We report a timeout or a leftover process with its PID, and leave the
    process running so that someone can inspect it.
    """
    if not menu_shots.sandboxed(app):
        raise SystemExit(f"{app.name} is not sandboxed; refusing to launch")
    log = menu_shots.log_of(app)
    if log and log.exists():
        log.unlink()
    forget_tray_record(app)
    inside = shot
    if shot is not None:
        shot.parent.mkdir(parents=True, exist_ok=True)
        shot.unlink(missing_ok=True)
        data = menu_shots.data_dir_of(app)
        if data is not None:
            inside = data / "shots" / shot.name
            inside.parent.mkdir(parents=True, exist_ok=True)
            inside.unlink(missing_ok=True)
    env = {
        **os.environ,
        "ROMINABOX_MAX_FRAMES": frame_limit(script),
        "ROMINABOX_MENU_SCRIPT": script,
        menu_shots.quiet_env(): "1",
    }
    if inside is not None:
        env["ROMINABOX_MENU_SHOT"] = str(inside.resolve())
    print(f"  {script}", flush=True)
    with tempfile.TemporaryFile(mode="w+t") as capture:
        process = subprocess.Popen(
            [str(menu_shots.launcher_of(app))],
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
    if inside is not None and shot is not None and inside != shot and inside.exists():
        shutil.move(str(inside), str(shot))
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
    return written


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
    if free_gb() < 22:
        raise SystemExit(f"disk has {free_gb()} GB free, below 22; stopping")
    print(f"export {design} {content.name}", flush=True)
    with menu_shots.build_a_game(
        content,
        PLAYER,
        system=system_for(content),
        design=design,
        palette=palette_id(),
    ) as app:
        yield app


def menu_asset(app: Path, name: str) -> str:
    path = app / "Contents/Resources/menu-assets" / name
    if not path.is_file():
        raise SystemExit(f"the export has no {name}")
    return path.read_text()


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


def button_top(menu: str, button_id: str) -> float:
    match = re.search(rf"<button\b[^>]*\bid=\"{re.escape(button_id)}\"[^>]*>", menu)
    if not match:
        raise SystemExit(f"the export has no {button_id} button")
    top = re.search(r"top:\s*(\d+(?:\.\d+)?)dp", match.group(0))
    if not top:
        raise SystemExit(f"the {button_id} entry does not say where it sits")
    return float(top.group(1))


def entry_box(css: str, menu: str, button_id: str, scale: float, width: int, height: int):
    origin_x, origin_y = screen_origin(css, scale, width, height)
    entries = rule_body(css, "#options-entries")
    entry = rule_body(css, ".option-entry")
    left = origin_x + dp(css_prop(entries, "left") or "0") * scale
    top = origin_y + (dp(css_prop(entries, "top") or "0") + button_top(menu, button_id)) * scale
    box_width = dp(css_prop(entry, "width") or "0") * scale
    box_height = dp(css_prop(entry, "height") or "0") * scale
    inset = border_dp(entry) * scale + 2
    return (
        int(left + inset),
        int(top + inset),
        int(left + box_width - inset),
        int(top + box_height - inset),
    )


def near(pixel: tuple[int, ...], colour: tuple[int, int, int]) -> bool:
    return all(abs(channel - wanted) <= 28 for channel, wanted in zip(pixel[:3], colour))


def button_fraction(image, box: tuple[int, int, int, int]) -> float:
    surface = palette_color("surface")
    highlight = palette_color("highlight")
    pixels = image.load()
    left, top, right, bottom = box
    seen = 0
    hit = 0
    for y in range(max(top, 0), min(bottom, image.size[1])):
        for x in range(max(left, 0), min(right, image.size[0])):
            seen += 1
            if near(pixels[x, y], surface) or near(pixels[x, y], highlight):
                hit += 1
    if seen == 0:
        return 0.0
    return hit / seen


def window_dp(app: Path) -> tuple[int, int]:
    text = menu_shots.plan_text(app)
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


def options_entry_visible(app: Path, shot: Path, button_id: str) -> bool:
    css = menu_asset(app, "menu.rcss")
    menu = menu_asset(app, "menu.rml")
    image = open_image(shot)
    window_width, _height = window_dp(app)
    scale = image.size[0] / window_width
    box = entry_box(css, menu, button_id, scale, image.size[0], image.size[1])
    fraction = button_fraction(image, box)
    print(f"  {button_id} button pixels {fraction:.2f} in {box}")
    return fraction > 0.4


def exported_player(playlist: Path) -> list[str]:
    if free_gb() < 22:
        return [f"disk has {free_gb()} GB free, below 22; stopping"]
    fresh_player_dir()
    before = player_support.snapshot()
    found: list[str] = []
    exported_apps: list[Path] = []
    try:
        for design in disc_designs():
            with export_game(playlist, design) as app:
                exported_apps.append(app)
                written = launch(
                    app,
                    script_of(swap_steps(design), tray_countdown() + 40),
                    SHOTS / f"{design}-two-list.png",
                )
                problem = require_swap(design, written)
                if problem:
                    found.append(problem)
                    return found
                screens = design_screens(design)
                listing = listing_of(screens)
                if listing.get("option"):
                    shot = SHOTS / f"{design}-two-options.png"
                    launch(app, script_of([
                        next(
                            screen["button"]
                            for screen in screens
                            if screen.get("place") == "options"
                        )
                    ]), shot)
                    if not options_entry_visible(app, shot, listing["button"]):
                        found.append(f"{design}: a two-disc game does not show the Disc entry")
                        return found
        for design in disc_designs():
            with export_game(CARTRIDGE, design) as app:
                exported_apps.append(app)
                screens = design_screens(design)
                listing = listing_of(screens)
                redirect = redirect_of(screens, listing["id"])
                shot = SHOTS / f"{design}-one.png"
                launch(app, script_of(one_disc_steps(design)), shot)
                if listing.get("option"):
                    if options_entry_visible(app, shot, listing["button"]):
                        found.append("a one-disc game must not show the entry")
                        return found
                    controls = next(
                        screen for screen in screens if screen.get("id") == "controls"
                    )
                    if not options_entry_visible(app, shot, controls["button"]):
                        found.append(
                            "the options screen did not show CONTROLS, so the Disc slot was not measured"
                        )
                        return found
                if redirect is not None:
                    if not circle_open(open_image(shot), menu_asset(app, "menu.rcss"), app):
                        found.append("the DISC button did not open the circle")
                        return found
    finally:
        for app in exported_apps:
            left = still_running(app)
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
    return found


def main() -> None:
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

    problems = [item for item in (index_problem, *exported_player(two)) if item]
    if problems:
        raise SystemExit("\n".join(problems))


if __name__ == "__main__":
    main()
