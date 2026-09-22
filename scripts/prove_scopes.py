"""Break each scope on purpose and check that it fails for that reason.

A fast test that passes after we remove the code it should check is worse
than a slow one. For each scope we edit one file, run the scope, and put the
file back, also when the scope does not fail.

    python3 scripts/prove_scopes.py
    python3 scripts/prove_scopes.py states overlays

We unset the worktree namespace for the child process. With it set, the
exporter tests stop with an error before the defect, and a pass would mean
nothing.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import worktree  # noqa: E402


def replace(path: Path, old: str, new: str):
    raw = path.read_bytes()
    needle = old.encode()
    if needle not in raw:
        raise SystemExit(f"{path} has no {old!r} to break")
    path.write_bytes(raw.replace(needle, new.encode(), 1))
    return raw


def restore(path: Path, raw: bytes) -> None:
    path.write_bytes(raw)


def run_scope(name: str) -> tuple[int, str]:
    env = os.environ.copy()
    env.pop("ROMINABOX_GAME_BUNDLE_PREFIX", None)
    env["CARGO_TARGET_DIR"] = str(worktree.common_dir() / "shared-cargo-target")
    result = subprocess.run(
        [sys.executable, str(ROOT / "scripts/test.py"), name],
        cwd=ROOT,
        capture_output=True,
        text=True,
        errors="replace",
        env=env,
    )
    return result.returncode, result.stdout + result.stderr


def expect(name: str, code: int, output: str, needles: tuple[str, ...]) -> str | None:
    if code == 0:
        return "the scope passed"
    found = next((needle for needle in needles if needle in output), None)
    if found is None:
        tail = output.strip()[-1200:]
        return f"it failed, but not with {needles[0]!r}:\n{tail}"
    line = next(line.strip() for line in output.splitlines() if found in line)
    print(f"    {line[:300]}")
    return None


def case_catalog():
    path = ROOT / "integrations/consoles/megadrive/console.json"
    raw = replace(path, '"name": "Mega Drive / Genesis"', '"name": "Not the Mega Drive"')
    try:
        code, output = run_scope("catalog")
    finally:
        restore(path, raw)
    return expect("catalog", code, output, ("megadrive name",))


def case_exporter():
    path = ROOT / "integrations/designs/native/menu.rml"
    raw = replace(path, 'id="resume"', 'id="resume-moved"')
    try:
        code, output = run_scope("exporter")
    finally:
        restore(path, raw)
    return expect("exporter", code, output, ("menu.rml omits",))


def case_picture():
    path = ROOT / "integrations/consoles/nes/components/nestopia.json"
    raw = replace(path, '"value": "disabled"', '"value": "composite"')
    try:
        code, output = run_scope("picture")
    finally:
        restore(path, raw)
    return expect("picture", code, output, ("resampled a hard edge",))


def case_frontend():
    path = ROOT / "desktop/src/palette.test.ts"
    raw = path.read_bytes()
    path.write_bytes(raw + b'\nconst fasttestsProbe: number = "fasttests";\n')
    try:
        code, output = run_scope("frontend")
    finally:
        restore(path, raw)
    return expect("frontend", code, output, ("not assignable to type 'number'",))


def case_builder():
    path = ROOT / "integrations/shaders/catalog.json"
    raw = replace(path, '"name": "Scanlines"', '"name": "Not a shader"')
    try:
        code, output = run_scope("builder")
    finally:
        restore(path, raw)
    return expect(
        "builder",
        code,
        output,
        ("shader packaging is not on the menu step",),
    )


def case_menu():
    path = ROOT / "integrations/designs/native/menu.rml"
    raw = replace(path, 'id="resume"', 'id="resume-moved"')
    try:
        code, output = run_scope("menu")
    finally:
        restore(path, raw)
    return expect("menu", code, output, ("menu interaction CHANGED", "probe failed"))


def case_heldkey():
    path = ROOT / "vendor/retroarch/input/held_key_policy.c"
    raw = replace(
        path,
        "{\n   int edge_down = 0;",
        "{\n   return 0;\n   int edge_down = 0;",
    )
    try:
        code, output = run_scope("heldkey")
    finally:
        restore(path, raw)
    return expect("heldkey", code, output, ("did not fire",))


def case_staging():
    path = ROOT / "scripts/native_runtime/build-builder-macos.sh"
    raw = replace(path, "integrations/designs", "integrations/designs-missing")
    try:
        code, output = run_scope("staging")
    finally:
        restore(path, raw)
    return expect("staging", code, output, ("does not exist",))


def case_joypad():
    hid = ROOT / "desktop/src-tauri/resources/runtime/autoconfig/hid"
    profiles = sorted(hid.glob("*.cfg"))
    if not profiles:
        return f"no hid profile to move in {hid}"
    path = profiles[0]
    aside = path.with_name(path.name + ".prove-aside")
    path.rename(aside)
    try:
        code, output = run_scope("joypad")
    finally:
        if aside.exists():
            aside.rename(path)
    return expect("joypad", code, output, ("missing",))


def case_worktree():
    # In a worktree we skip the checks that apply only to the canonical
    # checkout. The check of where we look up the CLI still runs, and it is
    # the one we break here.
    path = ROOT / "scripts/built.py"
    raw = replace(
        path,
        'return Path(shared) if shared else ROOT / "desktop/src-tauri/target"',
        'return ROOT / "desktop/src-tauri/target"',
    )
    try:
        code, output = run_scope("worktree")
    finally:
        restore(path, raw)
    return expect(
        "worktree",
        code,
        output,
        ("the command line is looked for where cargo was redirected",),
    )


def case_bridge():
    path = ROOT / "vendor/retroarch/menu/drivers/rmlui_bridge.h"
    raw = replace(
        path,
        "   return RIB_RMLUI_ACTION_RESUME;",
        "   return RIB_RMLUI_ACTION_CONTROLS_BACK;",
    )
    try:
        code, output = run_scope("bridge")
    finally:
        restore(path, raw)
    return expect("bridge", code, output, ("toggle resumes from the main screen",))


def case_states():
    path = ROOT / "scripts/fixtures/menu-states.json"
    raw = path.read_bytes()
    data = json.loads(raw)
    data["states"]["options"]["set"] = data["states"]["pause-menu"]["set"]
    path.write_text(json.dumps(data, indent=2) + "\n")
    try:
        code, output = run_scope("states")
    finally:
        restore(path, raw)
    return expect("states", code, output, ("IDENTICAL",))


def case_fallback():
    """Remove one control from the grid we show for a console with no drawing.

    For a console without a controller illustration, that grid is the whole
    controls screen. We wrote this scope to catch a control without a box.
    Without the box, the player could not rebind that button, and we would
    show no error.
    """
    path = ROOT / "integrations/designs/native/menu.rml"
    raw = path.read_bytes()
    text = raw.decode()
    marker = "<!--CONTROLS-->"
    if marker not in text:
        return expect("fallback", 1, f"{marker} is not in the design's markup", (marker,))
    path.write_text(text.replace(marker, "<!--CONTROLS-REMOVED-->", 1))
    try:
        code, output = run_scope("fallback")
    finally:
        restore(path, raw)
    return expect("fallback", code, output, ("REFUSED", "did not render", "not in the document"))


def case_placement():
    path = ROOT / "scripts/fixtures/picker-coverage.json"
    raw = path.read_bytes()
    data = json.loads(raw)
    data["ps1"] = 0
    path.write_text(json.dumps(data, indent=2) + "\n")
    try:
        code, output = run_scope("placement")
    finally:
        restore(path, raw)
    return expect("placement", code, output, ("WORSE   ps1",))


def case_variants():
    path = ROOT / "desktop/assets/controllers/controller-megadrive6.png"
    aside = path.with_name(path.name + ".prove-aside")
    raw_existed = path.is_file()
    if not raw_existed:
        return f"{path.name} is not there to remove"
    path.rename(aside)
    try:
        code, output = run_scope("variants")
    finally:
        if aside.exists():
            aside.rename(path)
    return expect(
        "variants",
        code,
        output,
        ("its artwork is not staged", "Could not load texture", "could not stage controls"),
    )


def case_identification():
    path = ROOT / "desktop/src-tauri/src/artwork.rs"
    raw = replace(
        path,
        "!wanted.title.is_empty() && wanted.title == found.title",
        "!wanted.title.is_empty() && false",
    )
    try:
        code, output = run_scope("identification")
    finally:
        restore(path, raw)
    return expect("identification", code, output, ("handed",))


def case_automation():
    path = ROOT / ".githooks/pre-push"
    raw = replace(path, "--all", "--list")
    try:
        code, output = run_scope("automation")
    finally:
        restore(path, raw)
    return expect("automation", code, output, ("no longer runs the whole suite",))


def case_artwork():
    path = ROOT / "desktop/assets/controllers/controller-megadrive.svg"
    # With rsvg, the drawing follows the viewBox and not the width attribute,
    # so we change the viewBox to move the picture that we measure the button
    # anchors against.
    raw = replace(
        path,
        'viewBox="0 0 67.733333 67.733333"',
        'viewBox="0 0 40 67.733333"',
    )
    try:
        code, output = run_scope("artwork")
    finally:
        restore(path, raw)
    return expect("artwork", code, output, ("DRIFTED", "drawing moved"))


def case_shaderpreview():
    # Change the shader, so the picture of it must stop matching. An edit to
    # the recorded PNG would only show that we run the comparison. This shows
    # that we make the preview by running the shader.
    path = ROOT / "integrations/shaders/catalog.json"
    raw = replace(
        path,
        "colour *= mix(1.0, 0.45, line);",
        "colour *= mix(1.0, 0.05, line);",
    )
    try:
        code, output = run_scope("shaderpreview")
    finally:
        restore(path, raw)
    return expect("shaderpreview", code, output, ("DRIFTED", "pixels differ"))


def case_size():
    path = ROOT / "scripts/fixtures/size-budgets.json"
    raw = path.read_bytes()
    data = json.loads(raw)
    data["installed_bytes"] = 1
    path.write_text(json.dumps(data, indent=2) + "\n")
    try:
        code, output = run_scope("size")
    finally:
        restore(path, raw)
    return expect("size", code, output, ("exceeds",))


def case_overlays():
    path = ROOT / "desktop/controls.json"
    raw = path.read_bytes()
    data = json.loads(raw)
    data["profiles"][0]["controls"][0]["calloutX"] += 40
    path.write_text(json.dumps(data, indent=2) + "\n")
    try:
        code, output = run_scope("overlays")
    finally:
        restore(path, raw)
    return expect("overlays", code, output, ("CHANGED",))


CASES = {
    "catalog": case_catalog,
    "exporter": case_exporter,
    "picture": case_picture,
    "frontend": case_frontend,
    "builder": case_builder,
    "menu": case_menu,
    "heldkey": case_heldkey,
    "staging": case_staging,
    "joypad": case_joypad,
    "worktree": case_worktree,
    "bridge": case_bridge,
    "states": case_states,
    "fallback": case_fallback,
    "placement": case_placement,
    "variants": case_variants,
    "identification": case_identification,
    "automation": case_automation,
    "artwork": case_artwork,
    "shaderpreview": case_shaderpreview,
    "size": case_size,
    "overlays": case_overlays,
}


def main() -> int:
    wanted = sys.argv[1:] or list(CASES)
    unknown = [name for name in wanted if name not in CASES]
    if unknown:
        raise SystemExit(f"unknown scope(s): {', '.join(unknown)}")
    failed = []
    for name in wanted:
        print(f"\n=== {name} ===", flush=True)
        problem = CASES[name]()
        if problem:
            print(f"  NOT PROVED  {problem}")
            failed.append(name)
        else:
            print("  proved")
    # controls.json is compiled into rominabox-cli. In the overlays case we
    # edit that file and run a scope in which we rebuild the binary, because
    # an earlier case changed a Rust file and the binary looks out of date.
    # After we restore the JSON, the binary is newer than the file but still
    # has the edit. So rebuild from the current tree once every case has put
    # its file back.
    from built import cli
    cli(build=True)

    print()
    if failed:
        print(f"{len(failed)} scope(s) did not fail for the right reason: {', '.join(failed)}")
        return 1
    print(f"all {len(wanted)} scopes failed for the reason they exist")
    return 0


if __name__ == "__main__":
    sys.exit(main())
