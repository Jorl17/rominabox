//! We write the bindings of a stick into the exported game as a capture in the
//! game would store them. Each direction's key is under the position we read
//! it from (`input_player1_<position>`, as in `rib_host_write_bind`), where we
//! read the author's defaults in the menu. We also tell the menu where a moved
//! direction is, and write a remap that gives the core the stick from its new
//! place. The input that the core then receives from these lines is
//! `pad_positions::played`.
#![cfg(target_os = "macos")]

mod export_fixture;
mod support;

use export_fixture::{export_request, workspace};
use rominabox_engine::controls::Controls;
use rominabox_engine::packaging::ExportRequest;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// A PlayStation game from the stand-in kit. The core in the kit is an empty
/// file with the name of PCSX ReARMed's, and we never load it here.
fn playstation(root: &Path, controls: serde_json::Value) -> ExportRequest {
    let mut request = export_request(root);
    let core = &rominabox_engine::systems::find("ps1").unwrap().cores[0];
    fs::write(
        request
            .runtime_kit
            .join("cores")
            .join(core.artifact().expect("a macOS artifact")),
        b"core",
    )
    .unwrap();
    fs::write(request.runtime_kit.join("licenses").join(&core.license_file), b"licence").unwrap();
    fs::write(root.join("game.bin"), vec![0u8; 2048]).unwrap();
    let cue = root.join("game.cue");
    fs::write(&cue, "FILE \"game.bin\" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\n").unwrap();
    request.game.rom = cue;
    request.game.system = "ps1".into();
    request.game.title = "Stick Export".into();
    request.game.show_menu = true;
    // We write the author's defaults in the menu, so the kit contains it.
    support::with_menu_assets(&request.runtime_kit);
    request.game.controls = serde_json::from_value::<Controls>(controls).unwrap();
    request
}

fn exported(request: &ExportRequest) -> PathBuf {
    rominabox_engine::packaging::export_game(request, &AtomicBool::new(false), |_| {})
        .unwrap()
        .app_path
}

/// The one file under `app` named `name`.
fn found(app: &Path, name: &str) -> String {
    fn walk(folder: &Path, name: &str, into: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, name, into);
            } else if path.file_name().is_some_and(|file| file == name) {
                into.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(app, name, &mut paths);
    assert_eq!(paths.len(), 1, "{name} in {}: {paths:?}", app.display());
    fs::read_to_string(&paths[0]).unwrap()
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

/// In a game's remap, pads 2 to 8 control player 1. This is the default
/// unless the author turns it off.
fn every_pad_player_one() -> String {
    (2..=rominabox_engine::pad_positions::PADS)
        .map(|pad| format!("input_remap_port_p{pad} = \"0\"\n"))
        .collect()
}

#[test]
fn a_stick_bound_to_keys_reaches_the_game_as_its_capture_stores_them() {
    let root = workspace();
    let request = playstation(
        &root,
        serde_json::json!({ "bindings": {
            "l_y_minus": { "key": "y" },
            "l_x_plus": { "key": "m" },
            "l_y_plus": { "key": "n" },
            "l_x_minus": { "key": "h" },
            "l3": { "key": "u" }
        }}),
    );
    let app = exported(&request);
    let defaults = found(&app, "controls-defaults.cfg");
    let written = lines(&defaults);
    for line in [
        "input_player1_l_y_minus = \"y\"",
        "input_player1_l_x_plus = \"m\"",
        "input_player1_l_y_plus = \"n\"",
        "input_player1_l_x_minus = \"h\"",
        "input_player1_l3 = \"u\"",
        "rib_group_l_y_minus = \"l_stick\"",
        "rib_label_l_x_minus = \"Left stick left\"",
    ] {
        assert!(written.contains(&line), "no {line} in\n{defaults}");
    }
    assert!(!defaults.contains("rib_position_"), "keys move nothing:\n{defaults}");
    assert_eq!(
        found(&app, "PCSX-ReARMed.rmp"),
        "input_libretro_device_p1 = \"517\"\n".to_string() + &every_pad_player_one(),
        "the remap carries the DualShock and nothing moved"
    );
}

#[test]
fn a_stick_moved_onto_the_d_pad_reaches_the_game_with_its_remap() {
    let root = workspace();
    let request = playstation(
        &root,
        serde_json::json!({ "bindings": {
            "l_y_minus": { "pad": "up" }, "up": { "pad": "l_y_minus" },
            "l_x_plus": { "pad": "right" }, "right": { "pad": "l_x_plus" },
            "l_y_plus": { "pad": "down" }, "down": { "pad": "l_y_plus" },
            "l_x_minus": { "pad": "left" }, "left": { "pad": "l_x_minus" }
        }}),
    );
    let app = exported(&request);
    let defaults = found(&app, "controls-defaults.cfg");
    let written = lines(&defaults);
    for line in [
        // Left stick up's key, T, is on D-pad up now, and D-pad up's on the stick.
        "input_player1_up = \"t\"",
        "rib_position_l_y_minus = \"up\"",
        "input_player1_l_y_minus = \"up\"",
        "rib_position_up = \"l_y_minus\"",
        "input_player1_left = \"v\"",
        "rib_position_l_x_minus = \"left\"",
    ] {
        assert!(written.contains(&line), "no {line} in\n{defaults}");
    }
    let remap = found(&app, "PCSX-ReARMed.rmp");
    let remapped = lines(&remap);
    assert!(remap.starts_with(&("input_libretro_device_p1 = \"517\"\n".to_string() + &every_pad_player_one())));
    // We move the stick on every pad, each with its own lines.
    for pad in 1..=rominabox_engine::pad_positions::PADS {
        for line in [
            format!("input_player{pad}_btn_up = \"19\""),
            format!("input_player{pad}_stk_l_y- = \"4\""),
            format!("input_player{pad}_btn_left = \"17\""),
            format!("input_player{pad}_stk_l_x- = \"6\""),
        ] {
            assert!(remapped.contains(&line.as_str()), "no {line} in\n{remap}");
        }
        let own = format!("input_player{pad}_");
        assert_eq!(remapped.iter().filter(|line| line.starts_with(&own)).count(), 8, "{remap}");
    }
    assert_eq!(remapped.len(), 1 + 7 + 8 * 8, "{remap}");
}

#[test]
fn half_a_stick_moved_is_refused_before_anything_is_built() {
    let root = workspace();
    let request = playstation(
        &root,
        serde_json::json!({ "bindings": {
            "l_y_minus": { "pad": "up" }, "up": { "pad": "l_y_minus" }
        }}),
    );
    let error = rominabox_engine::packaging::export_game(&request, &AtomicBool::new(false), |_| {})
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Left stick down stops working while Left stick up is moved"),
        "{error}"
    );
    assert!(!request.output_dir.join("Stick Export.app").exists());
}
