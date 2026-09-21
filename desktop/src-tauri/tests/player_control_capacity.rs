//! The player must be able to reach every control a console declares.
//!
//! A PlayStation DualShock declares twenty-four controls, including eight
//! analogue directions. In the player we read the control list from the
//! exported configuration, generated from the catalog, and resolve each id
//! against `input_config_bind_map` in RetroArch, so analogue directions
//! resolve to `RARCH_ANALOG_*` binds and not `RETRO_DEVICE_ID_JOYPAD_*` ones.
//! These tests check that nobody adds a fixed-size control table again.
//!
//! They do NOT prove that a reachable control binds. They read source text
//! and generated data and do not run the menu.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn player_source(name: &str) -> Option<String> {
    std::fs::read_to_string(repo_root().join("vendor/retroarch/menu/drivers").join(name)).ok()
}

fn largest_declared_profile() -> (String, usize) {
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../../controls.json")).expect("controls registry");
    registry["profiles"]
        .as_array()
        .expect("profiles")
        .iter()
        .map(|profile| {
            (
                profile["id"].as_str().unwrap_or_default().to_string(),
                profile["controls"].as_array().map(Vec::len).unwrap_or(0),
            )
        })
        .max_by_key(|(_, count)| *count)
        .expect("at least one profile")
}

#[test]
fn the_player_can_hold_every_control_the_largest_console_declares() {
    let Some(source) = player_source("rmlui.c") else {
        eprintln!("vendor/retroarch is not checked out; nothing was verified");
        return;
    };
    let marker = "#define RIB_CONTROL_MAX ";
    let capacity: usize = source
        .find(marker)
        .map(|at| &source[at + marker.len()..])
        .and_then(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
        .expect("the player declares a control capacity");

    let (profile, declared) = largest_declared_profile();
    assert!(
        declared <= capacity,
        "'{profile}' declares {declared} controls and the player can hold \
         {capacity}. Raise RIB_CONTROL_MAX — it is a buffer bound, not a \
         vocabulary, and exceeding it is reported rather than truncated."
    );
}

#[test]
fn neither_the_player_nor_the_bridge_keeps_its_own_control_vocabulary() {
    let (Some(player), Some(bridge)) = (player_source("rmlui.c"), player_source("rmlui_bridge.cpp"))
    else {
        eprintln!("vendor/retroarch is not checked out; nothing was verified");
        return;
    };
    // The exact loop forms of a fixed bound, matched literally and not as a
    // paraphrase.
    assert!(
        !player.contains("rib_control_t rib_controls["),
        "rmlui.c has a static control table again; the list belongs to the \
         exported configuration, which the catalog generates"
    );
    assert!(
        !bridge.contains("const char *control_ids[]"),
        "rmlui_bridge.cpp has a control id array again; it should ask \
         rib_rmlui_control_id instead of knowing names"
    );
    // Loop syntax, not prose, so a comment that quotes the bound does not
    // fail the test.
    assert!(
        !bridge.contains("; index < 16;"),
        "rmlui_bridge.cpp walks a literal sixteen controls again"
    );
}

#[test]
fn navigation_order_is_not_hardcoded_per_console() {
    let Some(player) = player_source("rmlui.c") else {
        eprintln!("vendor/retroarch is not checked out; nothing was verified");
        return;
    };
    // Declaration order is the navigation order, so a new console requires
    // no code here.
    for table in ["megadrive_order", "gameboy_order"] {
        assert!(
            !player.contains(table),
            "{table} is back; navigation order comes from the declaration"
        );
    }
}
