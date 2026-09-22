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

use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

// We run the tests in parallel, and two of them export the same palette. With
// a directory named only after the palette, each test would read what the
// other had half written.
static NEXT: AtomicU64 = AtomicU64::new(0);

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rominabox-palette-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
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
