//! What the Options screen contains in a composed menu, for every design.
//!
//! We compose with `themes::compose_menu`, as in an export, from a kit with
//! the shipped designs and controller artwork.

mod support;

use std::{collections::BTreeMap, fs};
use support::{compose, declared_screens, designs, hovered, kit, showing};

/// When an author turns every Options entry off, there is no Options screen.
/// A cartridge has one disc, so there is no disc entry either.
#[test]
fn an_empty_options_list_ships_no_options_screen() {
    let root = rominabox_scratch::Scratch::dir("rominabox-empty-options");
    let kit = kit(&root);
    for design in designs() {
        let composed = compose(&kit, &design, Some(&[]), 1, &root.join(&design));
        let screens = declared_screens(&composed.cfg);
        assert!(
            !screens.iter().any(|id| id == "options"),
            "{design}: an empty list still declares an Options screen: {screens:?}"
        );
        for id in ["options-panel", "options", "discs", "discs-panel"] {
            assert!(
                !composed.menu.contains(&format!("id=\"{id}\"")),
                "{design}: an empty Options list still ships #{id}"
            );
        }
    }
}

/// The disc entry and its list exist only when the game has more than one
/// disc. A cartridge's default Options contains its default entries and no DISC.
#[test]
fn a_one_disc_game_has_no_disc_entry() {
    let root = rominabox_scratch::Scratch::dir("rominabox-one-disc");
    let kit = kit(&root);
    for design in designs() {
        let composed = compose(&kit, &design, None, 1, &root.join(&design));
        let screens = declared_screens(&composed.cfg);
        assert!(
            !screens.iter().any(|id| id == "discs"),
            "{design}: a one-disc game declares the disc list: {screens:?}"
        );
        for id in ["discs", "discs-panel"] {
            assert!(
                !composed.menu.contains(&format!("id=\"{id}\"")),
                "{design}: a one-disc game ships #{id}"
            );
        }
    }
}

/// A game of several discs has the disc list, whatever the author chose, and
/// in Native the player reaches it from Options.
#[test]
fn a_game_of_several_discs_gets_the_disc_list() {
    let root = rominabox_scratch::Scratch::dir("rominabox-several-discs");
    let kit = kit(&root);
    for design in designs() {
        for entries in [None, Some(Vec::new())] {
            let destination = root.join(format!("{design}-{}", entries.is_some()));
            let composed = compose(&kit, &design, entries.as_deref(), 3, &destination);
            let screens = declared_screens(&composed.cfg);
            assert!(
                screens.iter().any(|id| id == "discs"),
                "{design}: three discs and no disc list: {screens:?}"
            );
            assert!(composed.menu.contains("id=\"discs-panel\""), "{design}");
            if design == "native" {
                assert!(
                    screens.iter().any(|id| id == "options"),
                    "native reaches the disc list from Options: {screens:?}"
                );
                assert!(composed.menu.contains("id=\"discs\""), "the DISC entry");
            }
        }
    }
}

/// Options entries do not overlap. In the RmlUi layout, the next entry covers
/// the lower edge of an entry that overlaps it, and navigation by layout can
/// never reach the covered one. Every entry comes from the same template, so
/// each must be under the pointer for the same height.
#[test]
fn options_entries_do_not_overlap() {
    let root = rominabox_scratch::Scratch::dir("rominabox-options-overlap");
    let kit = kit(&root);
    let entries: Vec<String> = ["controls", "video", "shaders", "achievements"]
        .into_iter()
        .map(str::to_string)
        .collect();
    for design in designs() {
        let destination = root.join(&design);
        let composed = compose(&kit, &design, Some(&entries), 1, &destination);
        let document = destination.join("options.rml");
        fs::write(&document, showing(&composed.menu, "options-panel")).unwrap();
        let column: Vec<(i32, i32)> = (0..600).map(|y| (480, y)).collect();
        // SHADERS is on VIDEO, and the others are rows of Options.
        let in_options: Vec<&String> = entries.iter().filter(|entry| *entry != "shaders").collect();
        let mut rows: BTreeMap<String, usize> = BTreeMap::new();
        for hover in hovered(&document, &column) {
            if in_options.contains(&&hover) {
                *rows.entry(hover).or_default() += 1;
            }
        }
        assert_eq!(
            rows.len(),
            in_options.len(),
            "{design}: entries under the pointer: {rows:?}"
        );
        let heights: Vec<usize> = in_options.iter().map(|entry| rows[*entry]).collect();
        assert!(
            heights.iter().all(|height| *height == heights[0]),
            "{design}: each entry is under the pointer for a different height, so one \
             covers another: {rows:?}"
        );
    }
}
