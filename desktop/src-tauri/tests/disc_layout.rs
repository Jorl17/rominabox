//! The place of BACK in the Disc design, from the RmlUi element boxes.
//!
//! BACK is in the same place on every Disc screen that has it, as on Index
//! (Options), so a player who has learnt where it is on one screen does not
//! have to look for it on the next. The volume row on Index is 24dp above
//! BACK and 24dp to its left, so the row is not in the column of BACK.
//!
//! The menu is the one we compose for a full export (every Options entry,
//! filters, achievements and several discs), and we lay it out with the
//! windowless RmlUi probe at the two window widths a player most often sees.
//! This does NOT prove that the place is good or how BACK looks. For that, we
//! draw the screens in the picture tests.

mod support;

use std::fs;
use support::{boxes, compose_with, kit, showing};

const SIZES: [(u32, u32); 2] = [(960, 600), (1280, 600)];
const GAP: f64 = 24.0;

#[test]
fn back_is_in_one_place_on_every_disc_screen_and_the_volume_keeps_clear_of_it() {
    let scratch = rominabox_scratch::Scratch::dir("rominabox-disc-layout");
    let kit = kit(&scratch);
    let entries: Vec<String> = ["controls", "shaders", "achievements"]
        .map(String::from)
        .to_vec();
    let composed = compose_with(
        &kit,
        "disc",
        "megadrive",
        Some(&entries),
        3,
        &scratch.join("composed"),
    );
    // Every screen whose BACK is `<screen>-back` inside `<screen>-panel`.
    let mut screens: Vec<String> = composed
        .menu
        .match_indices("-back\"")
        .filter_map(|(end, _)| {
            let start = composed.menu[..end].rfind("id=\"")? + 4;
            Some(composed.menu[start..end].to_string())
        })
        .filter(|screen| composed.menu.contains(&format!("id=\"{screen}-panel\"")))
        .collect();
    screens.sort();
    screens.dedup();
    for wanted in ["options", "shaders", "controls", "achievements", "disc", "discs"] {
        assert!(
            screens.iter().any(|screen| screen == wanted),
            "the composed Disc menu has no {wanted} screen with BACK: {screens:?}"
        );
    }

    let mut problems = Vec::new();
    for size in SIZES {
        let mut found = Vec::new();
        for screen in &screens {
            let document = scratch.join("composed").join(format!("show-{screen}.rml"));
            fs::write(&document, showing(&composed.menu, &format!("{screen}-panel"))).unwrap();
            let back = format!("{screen}-back");
            let measured = boxes(&document, size, &[&back, "volume-control"]);
            let back_box = measured[0].unwrap_or_else(|| panic!("{back} is not laid out"));
            found.push((screen.clone(), back_box));
            if screen == "options" {
                let volume = measured[1].expect("Index has a volume row");
                let [vx, vy, vw, vh] = volume;
                let [bx, by, _, _] = back_box;
                if vy + vh + GAP > by {
                    problems.push(format!(
                        "{size:?}: the volume row {volume:?} ends {}dp above BACK {back_box:?}, not {GAP}",
                        by - (vy + vh)
                    ));
                }
                if vx + vw + GAP > bx {
                    problems.push(format!(
                        "{size:?}: the volume row {volume:?} ends {}dp left of BACK {back_box:?}, not {GAP}: it reaches BACK's column",
                        bx - (vx + vw)
                    ));
                }
            }
        }
        let index = found
            .iter()
            .find(|(screen, _)| screen == "options")
            .map(|(_, back)| *back)
            .unwrap();
        for (screen, back) in &found {
            if *back != index {
                problems.push(format!(
                    "{size:?}: {screen}'s BACK is {back:?}, Index's is {index:?}"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
