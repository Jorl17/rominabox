//! Nothing reaches the game from a RetroPad button or stick direction that
//! no control of the game uses. By default RetroArch binds keys to some of
//! them (S to X, A to Y, Q to L, W to R), and the positions of a pad go to
//! the core unchanged without a remap. Gambatte reads X and Y as Turbo A and
//! Turbo B, so in a Game Boy game, holding S pressed A over and over.
#![cfg(target_os = "macos")]

mod export_fixture;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use export_fixture::{export_request, workspace};

/// The files under `app` whose names end with `suffix`.
fn files_ending(app: &Path, suffix: &str) -> Vec<PathBuf> {
    fn walk(folder: &Path, suffix: &str, into: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, suffix, into);
            } else if path.to_string_lossy().ends_with(suffix) {
                into.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(app, suffix, &mut found);
    found
}

#[test]
fn a_game_boy_game_s_unused_buttons_reach_nothing_from_the_keyboard_or_a_pad() {
    let root = workspace();
    let mut request = export_request(&root);
    let core = &rominabox_engine::systems::find("gbc").unwrap().cores[0];
    fs::write(request.runtime_kit.join("cores").join(core.artifact().unwrap()), b"core").unwrap();
    fs::write(request.runtime_kit.join("licenses").join(&core.license_file), b"licence").unwrap();
    fs::write(root.join("game.gbc"), vec![0u8; 32768]).unwrap();
    request.game.rom = root.join("game.gbc");
    request.game.system = "gbc".into();
    let app = rominabox_engine::packaging::export_game(&request, &AtomicBool::new(false), |_| {})
        .unwrap()
        .app_path;

    let defaults = files_ending(&app, "controls-defaults.cfg");
    let defaults = fs::read_to_string(&defaults[0]).unwrap();
    for button in ["x", "y", "l", "r", "l2", "r2", "l3", "r3"] {
        let line = format!("input_player1_{button} = \"nul\"");
        assert!(defaults.lines().any(|written| written == line), "no {line} in\n{defaults}");
    }

    let remaps = files_ending(&app, ".rmp");
    assert_eq!(remaps.len(), 1, "{remaps:?}");
    let remap = fs::read_to_string(&remaps[0]).unwrap();
    for pad in 1..=rominabox_engine::pad_positions::PADS {
        for position in ["btn_x", "btn_y", "btn_l", "btn_r", "stk_l_x+", "stk_r_y-"] {
            let line = format!("input_player{pad}_{position} = \"-1\"");
            assert!(remap.lines().any(|written| written == line), "no {line} in\n{remap}");
        }
    }
}
