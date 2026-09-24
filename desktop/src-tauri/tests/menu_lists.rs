//! The list screens we generate, for every design, with the RmlUi layout.

mod support;

use std::fs;
use support::{compose, designs, hovered, kit, showing};

/// The player can click every button in the action row of a list screen.
/// When two strips share one box, only the buttons of the later strip get
/// the clicks, so clicking the achievements session buttons (TURN OFF and
/// SIGN OUT) would do nothing while BACK works.
#[test]
fn every_list_action_can_be_hit_by_the_pointer() {
    let root = rominabox_scratch::Scratch::dir("rominabox-list-actions");
    let kit = kit(&root);
    let entries = vec!["controls".to_string(), "achievements".to_string()];
    // Move the pointer over the whole screen every 12 pixels, which hits
    // several points of every button, which is at least 36dp tall.
    let grid: Vec<(i32, i32)> = (0..50)
        .flat_map(|row| (0..80).map(move |column| (column * 12 + 6, row * 12 + 6)))
        .collect();
    for design in designs() {
        let destination = root.join(&design);
        let composed = compose(&kit, &design, Some(&entries), 1, &destination);
        // Signed in, we show the session buttons and hide RETRY.
        let signed_in = showing(&composed.menu, "achievements-panel").replacen(
            "id=\"achievements-session-actions\" class=\"list-actions account-session-actions\" style=\"display:none;\"",
            "id=\"achievements-session-actions\" class=\"list-actions account-session-actions\"",
            1,
        ).replacen(
            "id=\"achievements-session-actions\" class=\"account-session-actions\" style=\"display:none;\"",
            "id=\"achievements-session-actions\" class=\"account-session-actions\"",
            1,
        );
        let document = destination.join("achievements.rml");
        fs::write(&document, signed_in).unwrap();
        let hits = hovered(&document, &grid);
        for id in [
            "achievements-enabled",
            "achievements-signout",
            "achievements-back",
        ] {
            assert!(
                hits.iter().any(|hit| hit == id),
                "{design}: the pointer cannot reach #{id} anywhere on the screen; it hits {:?}",
                hits.iter()
                    .filter(|hit| hit.starts_with("achievements"))
                    .collect::<std::collections::BTreeSet<_>>()
            );
        }
    }
}
