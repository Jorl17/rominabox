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
    chord = ROOT / "vendor/retroarch/input/alt_enter_fullscreen.c"
    raw = replace(
        path,
        "{\n   int edge_down = 0;",
        "{\n   return 0;\n   int edge_down = 0;",
    )
    raw_chord = None
    try:
        raw_chord = replace(
            chord,
            "if (!(return_down && alt_held) || latched)\n      return 0;",
            "if (1)\n      return 0;",
        )
        code, output = run_scope("heldkey")
    finally:
        restore(path, raw)
        if raw_chord is not None:
            restore(chord, raw_chord)
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


def case_shotsign():
    path = ROOT / "scripts/menu_shots.py"
    raw = replace(
        path,
        '"--entitlements", str(entitlements),',
        '"--preserve-metadata=entitlements",',
    )
    try:
        code, output = run_scope("shotsign")
    finally:
        restore(path, raw)
    return expect("shotsign", code, output, ("app-sandbox",))


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


def case_edges():
    path = ROOT / "scripts/check_menu_edges.py"
    raw = replace(path, "HIGHLIGHT = (0xFF, 0xF1, 0x3D)", "HIGHLIGHT = (0x00, 0x00, 0x00)")
    try:
        code, output = run_scope("edges")
    finally:
        restore(path, raw)
    return expect("edges", code, output, ("no highlight outline",))


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
    """Break the PNG in each of the ways the scope checks, one at a time.

    The first is a PNG that no longer matches its SVG. The second is a PNG
    that someone edited by hand. Two encoders differ in antialiasing, so the
    render comparison allows a few percent of different pixels, and a
    repainted button is within that. We find the edit only in the bytes.
    """
    svg = ROOT / "desktop/assets/controllers/controller-megadrive.svg"
    # With rsvg, the drawing follows the viewBox and not the width attribute,
    # so we change the viewBox to move the picture that we measure the button
    # anchors against.
    raw = replace(
        svg,
        'viewBox="0 0 67.733333 67.733333"',
        'viewBox="0 0 40 67.733333"',
    )
    try:
        code, output = run_scope("artwork")
    finally:
        restore(svg, raw)
    drift = expect("artwork", code, output, ("DRIFTED", "drawing moved"))
    if drift:
        return drift

    # One pixel, far below every tolerance in the render comparison.
    png = ROOT / "desktop/assets/controllers/controller-nes.png"
    before = png.read_bytes()
    try:
        from PIL import Image

        image = Image.open(png).convert("RGBA")
        image.putpixel((900, 400), (255, 0, 0, 255))
        image.save(png)
        code, output = run_scope("artwork")
    finally:
        png.write_bytes(before)
    return expect("artwork", code, output, ("EDITED",))


def case_glslcore():
    # Give a 3.2 core context GLSL 130, a version that fails to compile on
    # macOS. The check for a NULL path is the other half of the fix, and this
    # break covers the version number.
    path = ROOT / "vendor/retroarch/gfx/drivers_shader/shader_glsl.c"
    raw = replace(path, "      return 150;", "      return 130;")
    try:
        code, output = run_scope("glslcore")
    finally:
        restore(path, raw)
    return expect("glslcore", code, output, ("a 3.2 core context gets GLSL 150",))


def case_padbinds():
    # Read only the binds from the configuration. We then show nothing for a
    # pad bound by an autoconfig profile, although its buttons work.
    path = ROOT / "vendor/retroarch/menu/drivers/rmlui.c"
    raw = replace(
        path,
        "   const struct retro_keybind *automatic = &input_autoconf_binds[0][index];",
        "   const struct retro_keybind *automatic = &input_autoconf_binds[0][index];\n"
        "   automatic = bind;",
    )
    try:
        code, output = run_scope("padbinds")
    finally:
        restore(path, raw)
    return expect("padbinds", code, output, ("autoconfig bound is listed",))


def case_menupreview():
    # Remove the element for the multi-bind list from the design, as in a
    # copy of the menu staged before that element existed.
    path = ROOT / "integrations/designs/native/menu.rml"
    raw = replace(path, "<!--BINDS-->", "")
    try:
        # In the builder we read the design from the kit, so we copy the broken
        # file into the kit, as staging would.
        kit = ROOT / "desktop/src-tauri/resources/runtime/designs/native/menu.rml"
        staged = kit.read_bytes()
        kit.write_bytes(path.read_bytes())
        try:
            code, output = run_scope("menupreview")
        finally:
            kit.write_bytes(staged)
    finally:
        restore(path, raw)
    return expect("menupreview", code, output, ("<!--BINDS-->", "cannot draw"))


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

def case_fixtures():
    path = ROOT / "scripts/fetch_test_content.py"
    raw = replace(path, "skipped: ", "quiet: ")
    try:
        code, output = run_scope("fixtures")
    finally:
        restore(path, raw)
    return expect("fixtures", code, output, ("not reported as skipped",))


def case_reporoot():
    # Put a compiled-in path back in the crate, so that in the catalog scope we
    # validate the packages of another checkout.
    path = ROOT / "desktop/crates/rominabox-catalog/src/lib.rs"
    raw = replace(
        path,
        'match std::env::var("ROMINABOX_REPO") {',
        'match std::env::var("SOMETHING_ELSE") {',
    )
    try:
        code, output = run_scope("reporoot")
    finally:
        restore(path, raw)
    return expect("reporoot", code, output, ("compiled in", "FAIL"))


def case_symlinks():
    """Break this scope on purpose.

    With a machine-local link into the canonical checkout, many scopes fail
    at once, so a check that cannot fail here is useless.

    We create nothing on disk. We read the index with `git ls-files -s`, so
    the break is an index entry of mode 120000 for a path that does not
    exist. There is never a link on the filesystem to follow, and we put it
    back with one index command.
    """
    path = ".rominabox-symlink-proof"
    blob = subprocess.run(
        ["git", "hash-object", "-w", "--stdin"],
        cwd=ROOT, input="desktop/node_modules\n", capture_output=True, text=True,
        check=True,
    ).stdout.strip()
    subprocess.run(
        ["git", "update-index", "--add", "--cacheinfo", f"120000,{blob},{path}"],
        cwd=ROOT, check=True, capture_output=True,
    )
    try:
        code, output = run_scope("symlinks")
    finally:
        subprocess.run(
            ["git", "update-index", "--force-remove", path],
            cwd=ROOT, check=True, capture_output=True,
        )
    return expect("symlinks", code, output, ("is a symbolic link",))


def case_isolation():
    path = ROOT / "desktop/src-tauri/src/packaging.rs"
    raw = replace(
        path,
        "com.apple.security.app-sandbox",
        "com.apple.security.sandbox-removed",
    )
    try:
        code, output = run_scope("isolation")
    finally:
        restore(path, raw)
    return expect("isolation", code, output, ("entitlements were dropped",))

def case_quiet():
    # Without the switch, the guardrail check must fail before we launch
    # anything, because a launch would open CoreAudio and play sound.
    path = ROOT / "scripts/menu_shots.py"
    raw = replace(path, '**{quiet_env(): "1"},\n', "")
    try:
        code, output = run_scope("quiet")
    finally:
        restore(path, raw)
    return expect("quiet", code, output, ("without ROMINABOX_QUIET",))


def case_quit():
    # In this scope we run menu_shots.built_player(), or the retroarch copied
    # from the runtime kit at export, and build no player. So with
    # NSTerminateNow back in ui_cocoa.m, we would see no failure here.
    import test_quit

    player = test_quit.launched_player()
    raw = player.read_bytes()
    marker = b"AppKit quit handed to orderly shutdown"
    if marker not in raw:
        raise SystemExit(f"{player} has no quit fix to revert")
    unfixed = pre_fix_player(player, marker)
    if unfixed is None:
        raise SystemExit(
            "the scope runs the frozen kit player, and this checkout has no "
            "earlier retroarch at work/quit-player-pre-fix"
        )
    player.write_bytes(unfixed)
    try:
        code, output = run_scope("quit")
    finally:
        player.write_bytes(raw)
    return expect("quit", code, output, ("quit aborted in the loaded core",))


def pre_fix_player(player: Path, marker: bytes) -> bytes | None:
    """Return a retroarch that still returns NSTerminateNow, if this checkout has one."""
    import menu_shots
    kit = menu_shots.KIT / "bin/retroarch"
    if kit != player and kit.is_file() and marker not in kit.read_bytes():
        return kit.read_bytes()
    saved = ROOT / "work/quit-player-pre-fix"
    if saved.is_file():
        data = saved.read_bytes()
        if marker not in data and len(data) > 1_000_000:
            return data
    return None


def case_shaderstate():
    # Return the unfiltered row whatever preset is running. In the check we
    # expect the row of a scanlines preset, and with this change we get the other.
    path = ROOT / "vendor/retroarch/menu/drivers/rmlui_shader_mark.h"
    raw = replace(path, "return found;", "return unfiltered;")
    try:
        code, output = run_scope("shaderstate")
    finally:
        restore(path, raw)
    return expect(
        "shaderstate",
        code,
        output,
        ("the row marked ON is not the shader that is running",),
    )


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
    "shotsign": case_shotsign,
    "bridge": case_bridge,
    "edges": case_edges,
    "states": case_states,
    "fallback": case_fallback,
    "placement": case_placement,
    "variants": case_variants,
    "identification": case_identification,
    "automation": case_automation,
    "artwork": case_artwork,
    "padbinds": case_padbinds,
    "glslcore": case_glslcore,
    "menupreview": case_menupreview,
    "shaderpreview": case_shaderpreview,
    "size": case_size,

    "reporoot": case_reporoot,
    "fixtures": case_fixtures,
    "symlinks": case_symlinks,
    "isolation": case_isolation,
    "overlays": case_overlays,
    "shaderstate": case_shaderstate,
    "quit": case_quit,
    "quiet": case_quiet,
}


def skipped_scopes() -> set[str]:
    """Return the scopes we run in test.py only when named. Proving one runs it."""
    import importlib.util
    spec = importlib.util.spec_from_file_location("rib_test_runner", ROOT / "scripts/test.py")
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    return {scope.name for scope in runner.SCOPES if scope.skipped}


def main() -> int:
    wanted = sys.argv[1:] or [name for name in CASES if name not in skipped_scopes()]
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
