//! The player must be able to reach every control a console declares.
//!
//! We expect this test to fail while the control cap below exists.
//!
//! In `vendor/retroarch/menu/drivers/rmlui_bridge.cpp` we list the control
//! ids in a C array and loop over it with a literal `index < 16`, bounded
//! by `RIB_RMLUI_ACTION_CONTROL_LAST = RIB_RMLUI_ACTION_CONTROL_FIRST + 15`.
//! The PlayStation DualShock declares 24 controls, fourteen buttons, two
//! stick clicks and eight analogue directions, so the player cannot reach,
//! focus or rebind the sticks in the menu at all, and we report no error
//! for this in the export.
//!
//! We read the cap from the vendored source, where it is defined. When the
//! array and the enum range are removed, delete this test and use
//! `scripts/menu_interaction.py` for the behaviour.
//!
//! This does NOT prove that a control which IS reachable works. We read
//! source text and do not run the menu.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn bridge_source(name: &str) -> Option<String> {
    let path: PathBuf = repo_root().join("vendor/retroarch/menu/drivers").join(name);
    std::fs::read_to_string(path).ok()
}

/// The literal bound in the loop over the control array.
fn addressable_controls(source: &str) -> Option<usize> {
    let marker = "const char *control_ids[] = {";
    let array_at = source.find(marker)?;
    let after = &source[array_at..];
    let loop_at = after.find("for (int index = 0; index <")?;
    let rest = &after[loop_at + "for (int index = 0; index <".len()..];
    let digits: String = rest
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn declared_controls(profile_id: &str) -> usize {
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../../controls.json")).expect("controls registry");
    registry["profiles"]
        .as_array()
        .expect("profiles")
        .iter()
        .find(|profile| profile["id"] == profile_id)
        .unwrap_or_else(|| panic!("no profile '{profile_id}'"))["controls"]
        .as_array()
        .expect("controls")
        .len()
}

#[test]
#[ignore = "fails while the control cap exists; run with --ignored"]
fn the_menu_can_address_every_control_the_playstation_pad_declares() {
    let Some(source) = bridge_source("rmlui_bridge.cpp") else {
        // The submodule is not always checked out. Then we skip and say so,
        // which is better than a green tick that checked nothing.
        eprintln!("vendor/retroarch is not checked out; nothing was verified");
        return;
    };
    let Some(addressable) = addressable_controls(&source) else {
        panic!(
            "could not find the control loop in rmlui_bridge.cpp. If the array \
             and its literal bound are gone, the defect this guards is fixed — \
             delete this test."
        );
    };
    let declared = declared_controls("ps1");
    assert!(
        declared <= addressable,
        "the PlayStation pad declares {declared} controls but the menu can \
         address only {addressable}. The analogue sticks are unreachable: they \
         are declared, exported and written into the remap, and the menu never \
         shows them."
    );
}

/// The same cap, as the enum range, so that we notice when the two differ.
#[test]
fn the_action_enum_range_matches_the_loop_bound() {
    let (Some(header), Some(source)) = (
        bridge_source("rmlui_bridge.h"),
        bridge_source("rmlui_bridge.cpp"),
    ) else {
        eprintln!("vendor/retroarch is not checked out; nothing was verified");
        return;
    };
    let Some(addressable) = addressable_controls(&source) else {
        return; // The other test reports this properly.
    };
    let Some(range_at) = header.find("RIB_RMLUI_ACTION_CONTROL_FIRST + ") else {
        return; // Enum range gone: the fix has landed.
    };
    let digits: String = header[range_at + "RIB_RMLUI_ACTION_CONTROL_FIRST + ".len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let last: usize = digits.parse().expect("an enum bound");
    assert_eq!(
        last + 1,
        addressable,
        "the enum reserves {} action ids but the loop walks {addressable}; \
         raising one without the other silently drops or invents controls",
        last + 1
    );
}
