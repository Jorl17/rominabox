//! The elements a menu design must contain for the player to work.
//!
//! In `rmlui_bridge.cpp` we look up elements in the document by id, and we
//! check every lookup for a missing element, for example
//!
//!     if (Rml::Element *hit = document->GetElementById("control-hit-" + suffix))
//!
//! So a design without an id does not fail. Part of it works without a
//! warning, and the fault appears somewhere unrelated, such as a button that
//! does nothing or a panel that never appears.
//!
//! We declare the contract here once and check it in both directions. Every
//! id that we look up in the bridge must be classified here, and every
//! classified id must be in the design for that mode.
//!
//! These tests do not check that the elements behave, are visible, are
//! styled or are connected to anything, only that they exist.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// A group of ids on one screen, and whether a design must have that screen.
///
/// We group the ids by panel and not in one flat list, because the author
/// chooses the sections of a game. A design without an optional panel must
/// stay valid, and to add a panel we add an entry here.
struct Panel {
    name: &'static str,
    required: bool,
    ids: &'static [&'static str],
}

/// The ids that the full menu document must contain.
///
/// In splash mode we stage `splash.rml` over `menu.rml`, so we always load one
/// file name in the bridge, and these are the ids we require when that file is
/// the menu.
const MENU_PANELS: &[Panel] = &[
    Panel {
        name: "shell",
        required: true,
        ids: &["body", "screen", "heading", "status", "footer-hint"],
    },
    Panel {
        name: "pause",
        required: true,
        ids: &["pause-panel", "load"],
    },
    Panel {
        name: "controls",
        required: true,
        ids: &["controls-panel", "controls-back", "controls-reset", "controls-cancel"],
    },
];

fn menu_ids(required_only: bool) -> Vec<&'static str> {
    MENU_PANELS
        .iter()
        .filter(|panel| panel.required || !required_only)
        .flat_map(|panel| panel.ids.iter().copied())
        .collect()
}

/// The ids that the splash document must contain, a much smaller set.
const SPLASH_IDS: &[&str] = &["body", "screen", "splash", "splash-logo"];

/// Ids that we make in the bridge by appending a control id or a slot number.
/// They cannot be literally in the design, because we generate the controls
/// into `<!--CONTROLS-->` from the console package, and the slots are numbered.
const GENERATED_PREFIXES: &[&str] = &[
    "control-",
    "control-binding-",
    "control-hit-",
    "control-label-",
    "slot-",
    "slot-image-",
    "slot-label-",
    "slot-state-",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn read(relative: &str) -> Option<String> {
    std::fs::read_to_string(repo_root().join(relative)).ok()
}

fn ids_in(document: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = document;
    while let Some(at) = rest.find("id=\"") {
        rest = &rest[at + 4..];
        if let Some(end) = rest.find('"') {
            found.insert(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    found
}

fn ids_the_bridge_looks_up(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find("GetElementById(\"") {
        rest = &rest[at + 16..];
        if let Some(end) = rest.find('"') {
            found.insert(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    found
}

/// We may look up only ids that this contract classifies.
///
/// Without this check the contract would go stale. A new `GetElementById` in
/// the bridge would add a requirement for every design, and nobody would know
/// about it until something stopped working without a warning.
#[test]
fn every_id_the_bridge_reaches_for_is_classified() {
    let Some(bridge) = read("vendor/retroarch/menu/drivers/rmlui_bridge.cpp") else {
        eprintln!("vendor/retroarch is not checked out; nothing was verified");
        return;
    };
    let classified: BTreeSet<&str> = menu_ids(false)
        .into_iter()
        .chain(SPLASH_IDS.iter().copied())
        .chain(GENERATED_PREFIXES.iter().copied())
        .collect();

    // We may also look up a generated id literally. For example, in the bridge
    // we measure `slot-image-1` to get the picture aspect. So we classify an id
    // that begins with a generated prefix by that prefix.
    let unclassified: Vec<String> = ids_the_bridge_looks_up(&bridge)
        .into_iter()
        .filter(|id| {
            !classified.contains(id.as_str())
                && !GENERATED_PREFIXES
                    .iter()
                    .any(|prefix| id.starts_with(prefix) && id.len() > prefix.len())
        })
        .collect();
    assert!(
        unclassified.is_empty(),
        "the bridge looks up ids this contract does not classify: {unclassified:?}. \
         Add each to MENU_IDS, SPLASH_IDS or GENERATED_PREFIXES, so every design \
         knows it has to provide them."
    );
}

#[test]
fn the_shipped_design_provides_every_menu_id() {
    let Some(document) = read("desktop/assets/menu/menu.rml") else {
        panic!("the shipped design has no menu.rml");
    };
    let present = ids_in(&document);
    for panel in MENU_PANELS.iter().filter(|panel| panel.required) {
        let missing: Vec<&&str> = panel.ids.iter().filter(|id| !present.contains(**id)).collect();
        assert!(
            missing.is_empty(),
            "menu.rml omits {missing:?} from the required '{}' panel. The bridge \
             looks these up defensively, so whatever they belong to does nothing \
             and reports nothing.",
            panel.name
        );
    }
}

#[test]
fn the_shipped_design_provides_every_splash_id() {
    let Some(document) = read("desktop/assets/menu/splash.rml") else {
        panic!("the shipped design has no splash.rml");
    };
    let present = ids_in(&document);
    let missing: Vec<&&str> = SPLASH_IDS
        .iter()
        .filter(|id| !present.contains(**id))
        .collect();
    assert!(missing.is_empty(), "splash.rml is missing {missing:?}");
}

/// The controls placeholder is part of the contract too.
///
/// It is not an id, so the checks above would miss it, and without it there
/// is no controller in the menu of a design.
#[test]
fn the_menu_document_has_somewhere_to_put_the_controls() {
    let Some(document) = read("desktop/assets/menu/menu.rml") else {
        panic!("the shipped design has no menu.rml");
    };
    assert!(
        document.contains("<!--CONTROLS-->"),
        "menu.rml has no <!--CONTROLS--> placeholder; the generated controller \
         scene has nowhere to go"
    );
}
