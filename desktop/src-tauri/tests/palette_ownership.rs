//! A palette gives colours to a design and changes nothing else in it.
//!
//! In an export we copy the design's stylesheet with the palette's colours in
//! its values. Rules added after the design's rules would declare selectors of
//! the design again and, with equal specificity, win because they come later.
//! An exported game would then look different from the design, for example
//! with the hover, keyboard focus and held-down states of the picker drawn
//! all alike.
//!
//! We check the file that we write in an export: it changes no style, and it
//! contains no colour from a palette other than the chosen one.

use std::{collections::BTreeSet, fs, path::PathBuf};

// We run the tests in parallel, and two of them export the same palette. With
// a directory named only after the palette, each test would read what the
// other had half written.

fn repo() -> PathBuf {
    rominabox_desktop::repo::root()
}

fn scratch(name: &str) -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir(&format!("rominabox-palette-{name}"))
}

fn exported_stylesheet(palette: &str) -> String {
    let design = repo().join("integrations/designs/native");
    let out = scratch(palette);
    rominabox_desktop::themes::prepare_theme_assets(&design, &out, palette, None)
        .unwrap_or_else(|e| panic!("staging the {palette} palette: {e}"));
    fs::read_to_string(out.join("menu.rcss")).unwrap()
}

/// Every selector in a stylesheet, in the order it appears.
fn selectors(css: &str) -> Vec<String> {
    let mut found = Vec::new();
    for block in css.split('}') {
        let Some(head) = block.split('{').next() else { continue };
        let head = head
            .lines()
            .filter(|line| !line.trim_start().starts_with('*') && !line.trim_start().starts_with("/*"))
            .collect::<Vec<_>>()
            .join(" ");
        let head = head.trim();
        if head.is_empty() || head.starts_with('@') {
            continue;
        }
        found.push(head.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    found
}

/// The stylesheet in an export is the design's, with other characters in its
/// values: the same rules and selectors, in the same order.
///
/// A palette gives only colours. Anything more would be a rule that is not in
/// the design, and because we would append it, it would win over the design.
/// For example, it could draw the three states of the picker alike, where the
/// design keeps them apart.
#[test]
fn an_export_is_the_design_with_its_values_filled_in() {
    let design = fs::read_to_string(repo().join("integrations/designs/native/menu.rcss")).unwrap();
    let authored = selectors(&design);
    let exported = selectors(&exported_stylesheet("blue"));

    assert_eq!(
        exported.len(),
        authored.len(),
        "an export has {} rules where the design has {}. The extra ones are \
         rules nobody authored, appended after the design's and therefore \
         winning over them.\n  added: {:?}",
        exported.len(),
        authored.len(),
        exported
            .iter()
            .filter(|s| !authored.contains(s))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        exported, authored,
        "an export reordered or renamed the design's rules; it may only change \
         what is inside them"
    );
}

/// When the author chooses green, there must be no blue in the file. Every
/// colour that a palette does not name comes from the design, and the colours
/// of the design are tokens, so no other colour can remain.
#[test]
fn no_other_palettes_colour_survives_an_export() {
    let registry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo().join("desktop/designs.json")).unwrap())
            .unwrap();
    let palettes = registry["palettes"].as_array().unwrap();

    for chosen in palettes {
        let id = chosen["id"].as_str().unwrap();
        let css = exported_stylesheet(id).to_lowercase();
        let mine: BTreeSet<String> = chosen
            .as_object()
            .unwrap()
            .values()
            .filter_map(|v| v.as_str())
            .filter(|v| v.starts_with('#'))
            .map(str::to_lowercase)
            .collect();

        for other in palettes {
            let other_id = other["id"].as_str().unwrap();
            if other_id == id {
                continue;
            }
            for (role, colour) in other.as_object().unwrap() {
                let Some(colour) = colour.as_str() else { continue };
                if !colour.starts_with('#') || mine.contains(&colour.to_lowercase()) {
                    continue;
                }
                assert!(
                    !css.contains(&colour.to_lowercase()),
                    "choosing {id} left {other_id}'s {role} colour {colour} in the \
                     exported stylesheet, so that part of the menu is drawn in a \
                     palette nobody chose"
                );
            }
        }
    }
}

/// Every colour that we write in an export comes from the chosen palette.
///
/// A design declares colours of its own, such as the frame around the
/// screen, the bevels on a save slot's picture and the greys of a disabled
/// button. A palette gives a value for every token of the design, so an
/// export in Green or Amber must have no blue frame, navy label strips or
/// blue disabled button around an amber menu.
///
/// We export in each palette and check that the palette declares every
/// colour in the result. We do not check whether a colour changes between
/// palettes, because the highlight is the same yellow in all three, and all
/// three declare it. Only the source of each colour counts.
#[test]
fn no_colour_survives_every_palette() {
    // White and black are not chosen in a palette. They are the ends of the
    // range, for a highlight edge and a shadow, and a palette with its own
    // would have a token for it.
    const NEUTRAL: [&str; 4] = ["#ffffff", "#000000", "#fff", "#000"];

    let declared: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo().join("desktop/designs.json")).unwrap())
            .unwrap();
    let palettes: Vec<String> = declared["palettes"]
        .as_array()
        .expect("palettes")
        .iter()
        .map(|entry| entry["id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        palettes.len() > 1,
        "one palette proves nothing about whether colour follows the palette"
    );

    for entry in declared["palettes"].as_array().unwrap() {
        let palette = entry["id"].as_str().unwrap();
        let mut allowed: BTreeSet<String> =
            NEUTRAL.iter().map(|c| c.to_string()).collect();
        for (name, value) in entry.as_object().unwrap() {
            if name == "tokens" {
                for value in value.as_object().unwrap().values() {
                    allowed.insert(value.as_str().unwrap().to_ascii_lowercase());
                }
            } else if let Some(value) = value.as_str() {
                if value.starts_with('#') {
                    allowed.insert(value.to_ascii_lowercase());
                }
            }
        }

        let written = hex_colours(&exported_stylesheet(palette));
        assert!(
            !written.is_empty(),
            "the {palette} export wrote a stylesheet with no colours in it"
        );
        let stray: Vec<String> = written.difference(&allowed).cloned().collect();
        assert!(
            stray.is_empty(),
            "the {palette} export writes {} colour(s) that palette never \
             declared: {}\n\
             A colour the design decides is a colour no scheme can change. \
             Declare a token for it in the design and give every palette a \
             value in desktop/designs.json.",
            stray.len(),
            stray.join(", ")
        );
    }
}

/// Every hex colour in a stylesheet, lowercased.
fn hex_colours(css: &str) -> BTreeSet<String> {
    let bytes = css.as_bytes();
    let mut found = BTreeSet::new();
    for (at, _) in css.match_indices('#') {
        let rest = &bytes[at + 1..];
        let len = rest
            .iter()
            .take(8)
            .take_while(|byte| byte.is_ascii_hexdigit())
            .count();
        if len == 3 || len == 6 || len == 8 {
            found.insert(css[at..at + 1 + len].to_ascii_lowercase());
        }
    }
    found
}
