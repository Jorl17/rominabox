//! We reserve only the keys of hotkeys used during play, and Escape is one of
//! them by default. Q and F are ordinary gameplay keys in every mode.
//!
//! Neither Q nor F is a RetroArch meta bind, with or without advanced
//! emulator access, because Quit is in the menu that the player opens with
//! Escape, and Alt+Enter is the fullscreen chord. The player opens
//! the menu with Escape because it is the default for MENU on the HOTKEYS
//! screen, which we read in the menu from `hotkeys-defaults.cfg` in the
//! export. RetroArch's own menu toggle has no key.
//!
//! In these tests we read generated config and the authoring validator. We
//! do not launch a player or open a window, and we do not prove that
//! pressing a key has the effect bound to it.

#[cfg(target_os = "macos")]
mod export_fixture;
#[cfg(target_os = "macos")]
#[path = "recovery_keys/macos.rs"]
mod macos;
#[cfg(target_os = "macos")]
mod support;

use rominabox_engine::{controls::Controls, meta_binds::isolated_meta_bind_config};

fn config_value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
    config.lines().find_map(|line| {
        let (name, value) = line.split_once(" = ")?;
        (name == key).then(|| value.trim_matches('"'))
    })
}

fn binding(key: &str) -> Controls {
    serde_json::from_value(serde_json::json!({
        "bindings": {"a": {"key": key}}
    }))
    .unwrap()
}

/// RetroArch's menu toggle has no key, with or without advanced access. The
/// player opens the menu with MENU on the HOTKEYS screen, Escape by default,
/// so Escape is not a gameplay binding.
///
/// The keyboard lines come from the meta bind policy in the launcher. This
/// does not prove that a keypress opens the menu. In the macOS export tests
/// (`recovery_keys/macos.rs`) we read a written launcher and menu.
#[test]
fn escape_toggles_the_menu_in_both_modes_and_is_never_a_gameplay_key() {
    for advanced in [false, true] {
        let config = isolated_meta_bind_config(advanced);
        assert_eq!(
            config_value(&config, "input_menu_toggle"),
            Some("nul"),
            "advanced={advanced}"
        );
    }
    let error = rominabox_engine::builder::unstated::hotkeys()
        .check_for("megadrive", &binding("escape"))
        .expect_err("escape stays reserved")
        .to_string();
    assert!(error.contains("bound to menu, which acts while the game plays"), "{error}");
}
