//! The checks that we run only on a Mac, of the launcher and menu in a macOS
//! export, which we write from the stand-in kit with a compiled runtime stub.

use super::config_value;
use crate::{export_fixture, support};
use rominabox_desktop::{controls::Controls, meta_binds::META_BINDS, packaging::ExportRequest};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn workspace() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-recovery")
}

/// The stand-in macOS kit with the Native design, the parts we compose it
/// with, and a picture for every pad that the player can choose.
fn fixture_kit(root: &Path) -> PathBuf {
    let kit = export_fixture::fixture_kit(root);
    let design = rominabox_desktop::repo::at("integrations/designs/native");
    support::copy_tree(&design, &kit.join("designs/native"));
    support::copy_tree(&rominabox_desktop::repo::at("integrations/parts"), &kit.join("parts"));
    fs::create_dir_all(kit.join("menu-assets")).unwrap();
    // We read the scene template for the controls from the kit's shared
    // menu-assets, not from the folder of the selected design.
    fs::copy(design.join("menu.rml"), kit.join("menu-assets/menu.rml")).unwrap();
    // Every pad that the player can choose in the picker, because an export
    // contains them all. Without a drawing for each, the player could choose a
    // pad, such as the six-button Mega Drive, that we would never show.
    for entry in rominabox_desktop::controls::variants_for_system("megadrive").unwrap() {
        if !entry.image.is_empty() {
            fs::write(kit.join("menu-assets").join(&entry.image), []).unwrap();
        }
    }
    fs::write(kit.join("menu-assets/CONTROLLERS.txt"), []).unwrap();
    kit
}

fn export_request(root: &Path, advanced: bool, controls: Controls) -> ExportRequest {
    let mut request = export_fixture::export_request_from(root, fixture_kit(root));
    request.game.title = "Recovery Keys".to_string();
    request.game.show_menu = true;
    request.game.controls = controls;
    request.game.advanced_emulator_access = advanced;
    request
}

fn embedded_runtime_config(plan: &str) -> String {
    let marker = "---config---\n";
    let start = plan
        .find(marker)
        .expect("exported launch plan contains the runtime config");
    plan[start + marker.len()..].to_string()
}

/// The bindings for MENU, as we write them in an export for the menu.
fn exported_menu(root: &Path, advanced: bool) -> String {
    let request = export_request(root, advanced, Controls::default());
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {})
        .unwrap_or_else(|error| panic!("advanced={advanced}: export refused: {error:?}"));
    let defaults = fs::read_to_string(
        result.app_path.join("Contents/Resources/menu-assets/hotkeys-defaults.cfg"),
    )
    .unwrap();
    config_value(&defaults, "hotkey_menu").unwrap().to_string()
}

/// The runtime config and the default controls that we write in an export.
fn export(advanced: bool, controls: Controls) -> (String, String) {
    let root = workspace();
    let request = export_request(&root, advanced, controls);
    let cancelled = AtomicBool::new(false);
    let result = rominabox_desktop::packaging::export_game(&request, &cancelled, |_| {})
        .unwrap_or_else(|error| panic!("advanced={advanced}: export refused: {error:?}"));
    let resources = result.app_path.join("Contents/Resources");
    let plan = fs::read_to_string(resources.join("launch.plan")).unwrap();
    let defaults = fs::read_to_string(resources.join("menu-assets/controls-defaults.cfg")).unwrap();
    (embedded_runtime_config(&plan), defaults)
}

fn exported_config(advanced: bool) -> String {
    export(advanced, Controls::default()).0
}

/// A on Q and B on F.
fn q_and_f() -> Controls {
    serde_json::from_value(serde_json::json!({
        "bindings": {"a": {"key": "q"}, "b": {"key": "f"}}
    }))
    .unwrap()
}

/// Every RetroArch meta bind whose keyboard key is `key`.
fn meta_binds_on<'a>(config: &'a str, key: &str) -> Vec<&'static str> {
    META_BINDS
        .iter()
        .map(|bind| bind.name)
        .filter(|name| config_value(config, &format!("input_{name}")) == Some(key))
        .collect()
}

/// In a default export we do not bind Q to quit, because a player could press
/// it by accident and lose the session. Quit is in the menu that Escape opens.
///
/// We read the launcher that an export writes. This does not prove that Quit
/// in the menu works, or that RetroArch receives a keypress.
#[test]
fn a_default_export_binds_no_exit_key() {
    let config = exported_config(false);
    assert_eq!(
        config_value(&config, "input_exit_emulator"),
        Some("nul"),
        "a shipped game must not quit on Q"
    );
    assert_eq!(
        config_value(&config, "input_menu_toggle"),
        Some("nul"),
        "RetroArch's own menu toggle opens nothing"
    );
    assert!(
        exported_menu(&workspace(), false).split(' ').any(|binding| binding == "key:escape"),
        "Escape is how the player reaches Quit"
    );
}

/// Q and F are gameplay keys in every mode. With advanced emulator access on
/// or off, we accept an export with A on Q and B on F, its controls contain
/// both keys, and no RetroArch hotkey is bound to either, so pressing either
/// key only acts in the game.
///
/// If we bound Q to quit and F to fullscreen with advanced access, a player of
/// an advanced export could not use them, or would quit the game mid-play.
///
/// We read what an export writes. This does not prove that RetroArch passes a
/// keypress to the core.
#[test]
fn q_and_f_are_gameplay_keys_and_no_meta_bind_in_every_mode() {
    for advanced in [false, true] {
        let (config, defaults) = export(advanced, q_and_f());
        assert_eq!(
            config_value(&defaults, "input_player1_a"),
            Some("q"),
            "advanced={advanced}"
        );
        assert_eq!(
            config_value(&defaults, "input_player1_b"),
            Some("f"),
            "advanced={advanced}"
        );
        for key in ["q", "f"] {
            assert_eq!(
                meta_binds_on(&config, key),
                Vec::<&str>::new(),
                "advanced={advanced}: {key} is a gameplay key and no meta bind"
            );
        }
        assert_eq!(config_value(&config, "input_exit_emulator"), Some("nul"));
        assert_eq!(
            config_value(&config, "input_toggle_fullscreen"),
            Some("nul")
        );
        assert_eq!(
            config_value(&config, "input_menu_toggle"),
            Some("nul"),
            "advanced={advanced}: RetroArch's own menu toggle opens nothing"
        );
        assert!(
            exported_menu(&workspace(), advanced)
                .split(' ')
                .any(|binding| binding == "key:escape"),
            "advanced={advanced}: Escape opens the menu, which has Quit"
        );
    }
}
